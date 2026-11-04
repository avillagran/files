//! Owned sync metadata artifacts, effect journal and explicit namespace primitives.
//! Parsing/renewal are not operator approval or current source authority.
//! Reopen loses durability state. Failed renewal ACK poisons that owner. Files
//! are never automatically replaced, removed, truncated or resumed.
use std::io;
pub const MAX_ARTIFACT: u64 = 24
    + crate::directory_sync_record::MAX_WIRE as u64
    + crate::directory_sync_plan::MAX_WIRE as u64
    + crate::directory_sync_metadata::MAX_STREAM_BYTES;
const MAGIC: &[u8; 8] = b"FLSMST\0\x01";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proof {
    bytes: [u8; 212],
}
#[derive(Clone, Debug)]
pub struct StoredProof {
    bytes: [u8; 212],
}
fn bad(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn number(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("fixed proof"))
}
impl Proof {
    pub fn encode(&self) -> [u8; 212] {
        self.bytes
    }
    pub fn directory_identity(&self) -> (u64, u64) {
        (number(&self.bytes, 8), number(&self.bytes, 16))
    }
    pub fn file_identity(&self) -> (u64, u64) {
        (number(&self.bytes, 24), number(&self.bytes, 32))
    }
    pub fn values_seal(&self) -> [u8; 32] {
        self.bytes[168..200].try_into().expect("fixed proof")
    }
}
impl StoredProof {
    /// Comparison data only. A decoded proof never grants durable/live authority.
    pub fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() != 212 || &bytes[..8] != MAGIC {
            return Err(bad("Formato de almacén inválido"));
        }
        let count = u32::from_le_bytes(
            bytes[208..212]
                .try_into()
                .map_err(|_| bad("Conteo inválido"))?,
        ) as usize;
        let len = number(bytes, 200);
        if number(bytes, 16) == 0
            || number(bytes, 32) == 0
            || number(bytes, 40) == 0
            || number(bytes, 40) != number(bytes, 48)
            || bytes[56..72] == [0; 16]
            || count > crate::directory_sync_metadata::MAX_RECORDS
            || len < 188 + 49 * count as u64
            || len > MAX_ARTIFACT
        {
            return Err(bad("Prueba de almacén inválida"));
        }
        Ok(Self {
            bytes: bytes.try_into().map_err(|_| bad("Prueba inválida"))?,
        })
    }
    pub fn matches(&self, live: &Proof) -> bool {
        self.bytes == live.bytes
    }
}
#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use crate::{
        directory_sync_manifest::Prepared,
        directory_sync_metadata::{self as metadata, Target, Values},
        directory_sync_physical::Physical,
        directory_sync_plan::Plan,
        directory_sync_record::Record,
        move_manifest::Kind,
    };
    use std::{
        ffi::{CString, OsString},
        fs::{File, Metadata},
        io::{Read, Seek, SeekFrom, Write},
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{
                ffi::{OsStrExt, OsStringExt},
                fs::MetadataExt,
            },
        },
        path::{Path, PathBuf},
        sync::atomic::{AtomicBool, Ordering},
    };
    /// Complete historical intent; reconstruct review without original paths.
    /// Decoded data cannot recreate Prepared pins, approval or durable barriers.
    pub struct SavedIntent {
        record: Record,
        plan: Vec<u8>,
        stage_limit: u64,
    }
    impl SavedIntent {
        pub fn record(&self) -> &Record {
            &self.record
        }
        pub fn plan(&self) -> io::Result<Plan<'_>> {
            Plan::decode(&self.record, &self.plan)
        }
        pub fn stage_limit(&self) -> u64 {
            self.stage_limit
        }
        fn prefix_len(&self) -> u64 {
            24 + self.record.bytes().len() as u64 + self.plan.len() as u64
        }
        fn matches(&self, physical: &Physical<'_, '_>) -> io::Result<bool> {
            Ok(self.record.bytes() == physical.plan().record().bytes()
                && self.plan == physical.plan().encode()?
                && self.stage_limit == physical.max_stage_payload())
        }
    }
    fn read_saved(reader: &mut impl Read) -> io::Result<SavedIntent> {
        let mut header = [0; 24];
        reader.read_exact(&mut header)?;
        if &header[..8] != b"FLSSAV\0\x01" {
            return Err(bad("Formato de intención guardada inválido"));
        }
        let n = u32::from_le_bytes(
            header[8..12]
                .try_into()
                .map_err(|_| bad("Longitud de evidencia inválida"))?,
        ) as usize;
        let m = u32::from_le_bytes(
            header[12..16]
                .try_into()
                .map_err(|_| bad("Longitud de plan inválida"))?,
        ) as usize;
        let stage_limit = u64::from_le_bytes(
            header[16..24]
                .try_into()
                .map_err(|_| bad("Presupuesto inválido"))?,
        );
        if n == 0
            || n > crate::directory_sync_record::MAX_WIRE
            || !(80..=crate::directory_sync_plan::MAX_WIRE).contains(&m)
            || stage_limit > crate::directory_sync_physical::MAX_STAGE_PAYLOAD
        {
            return Err(bad("Intención guardada excede límites"));
        }
        let mut bytes = vec![0; n];
        reader.read_exact(&mut bytes)?;
        let record = Record::from_owned(bytes)?;
        let mut plan = vec![0; m];
        reader.read_exact(&mut plan)?;
        Ok(SavedIntent {
            record,
            plan,
            stage_limit,
        })
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Node {
        device: u64,
        inode: u64,
        mount: u64,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Stamp {
        length: u64,
        modified: (i64, i64),
        changed: (i64, i64),
    }
    fn stamp(m: &Metadata) -> Stamp {
        Stamp {
            length: m.len(),
            modified: (m.mtime(), m.mtime_nsec()),
            changed: (m.ctime(), m.ctime_nsec()),
        }
    }
    fn owned(fd: i32) -> io::Result<File> {
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }
    fn c(bytes: &[u8]) -> io::Result<CString> {
        CString::new(bytes).map_err(|_| bad("NUL en ruta de almacén"))
    }
    fn node(file: &File) -> io::Result<Node> {
        let m = file.metadata()?;
        let mut s = std::mem::MaybeUninit::<libc::statx>::zeroed();
        if unsafe {
            libc::statx(
                file.as_raw_fd(),
                c"".as_ptr(),
                libc::AT_EMPTY_PATH | libc::AT_SYMLINK_NOFOLLOW,
                libc::STATX_MNT_ID,
                s.as_mut_ptr(),
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        let s = unsafe { s.assume_init() };
        if s.stx_mask & libc::STATX_MNT_ID == 0 || s.stx_mnt_id == 0 || m.ino() == 0 {
            return Err(bad("Identidad/montaje no disponible"));
        }
        Ok(Node {
            device: m.dev(),
            inode: m.ino(),
            mount: s.stx_mnt_id,
        })
    }
    fn private_directory(file: &File) -> io::Result<()> {
        let m = file.metadata()?;
        if !m.is_dir() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o7777 != 0o700 {
            return Err(bad(
                "El almacén requiere una carpeta propia y privada (0700)",
            ));
        }
        Ok(())
    }
    fn private_file(file: &File) -> io::Result<()> {
        let m = file.metadata()?;
        if !m.is_file()
            || m.uid() != unsafe { libc::geteuid() }
            || m.mode() & 0o7777 != 0o600
            || m.nlink() != 1
        {
            return Err(bad(
                "El registro debe ser un archivo propio y privado (0600), sin enlaces adicionales",
            ));
        }
        Ok(())
    }
    struct Directory {
        path: PathBuf,
        parts: Vec<OsString>,
        chain: Vec<Node>,
        file: File,
    }
    impl Directory {
        fn walk(parts: &[OsString], expected: Option<&[Node]>) -> io::Result<(File, Vec<Node>)> {
            let mut file = owned(unsafe {
                libc::open(
                    c"/".as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            })?;
            let mut chain = Vec::with_capacity(parts.len() + 1);
            chain.push(node(&file)?);
            if expected.is_some_and(|e| e.first() != chain.first()) {
                return Err(bad("Raíz del almacén fue sustituida"));
            }
            for (i, p) in parts.iter().enumerate() {
                file = owned(unsafe {
                    libc::openat(
                        file.as_raw_fd(),
                        c(p.as_bytes())?.as_ptr(),
                        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                    )
                })?;
                let n = node(&file)?;
                if expected.is_some_and(|e| e.get(i + 1) != Some(&n)) {
                    return Err(bad("Ancestro/montaje de almacén cambió"));
                }
                chain.push(n);
            }
            Ok((file, chain))
        }
        fn path_shape(path: &Path) -> io::Result<(usize, usize)> {
            let raw = path.as_os_str().as_bytes();
            if raw.first() != Some(&b'/') || raw.len() > 4096 || raw.contains(&0) {
                return Err(bad("Ruta absoluta de almacén inválida"));
            }
            let (mut count, mut bytes) = (0, 0);
            if raw != b"/" {
                for p in raw[1..].split(|&b| b == b'/') {
                    if p.is_empty() || p == b"." || p == b".." || p.len() > 255 {
                        return Err(bad("Componente de almacén inválido"));
                    }
                    count += 1;
                    bytes += p.len();
                }
            }
            if count > 128 {
                return Err(bad("Demasiados ancestros de almacén"));
            }
            Ok((count, bytes))
        }
        fn open(path: &Path) -> io::Result<Self> {
            let (count, _) = Self::path_shape(path)?;
            let raw = path.as_os_str().as_bytes();
            let mut parts = Vec::with_capacity(count);
            if raw != b"/" {
                for p in raw[1..].split(|&b| b == b'/') {
                    parts.push(OsString::from_vec(p.to_vec()));
                }
            }
            let (file, chain) = Self::walk(&parts, None)?;
            private_directory(&file)?;
            Ok(Self {
                path: path.into(),
                parts,
                chain,
                file,
            })
        }
        fn check(&self) -> io::Result<()> {
            private_directory(&self.file)?;
            let (current, _) = Self::walk(&self.parts, Some(&self.chain))?;
            private_directory(&current)?;
            if node(&self.file)? != *self.chain.last().expect("root chain") {
                return Err(bad("Pin de almacén cambió"));
            }
            Ok(())
        }
        fn disjoint(&self, physical: &Physical<'_, '_>) -> io::Result<()> {
            let view = physical.plan().record().view()?;
            let mine = self.chain.last().expect("root chain");
            for root in &view.roots {
                if root.entries.iter().any(|e| {
                    e.identity.kind == Kind::Directory
                        && (e.identity.device, e.identity.inode) == (mine.device, mine.inode)
                }) || root
                    .ancestors
                    .iter()
                    .any(|a| (a.device, a.inode) == (mine.device, mine.inode))
                    || self.chain.iter().any(|n| {
                        (n.device, n.inode)
                            == (
                                root.entries[0].identity.device,
                                root.entries[0].identity.inode,
                            )
                    })
                {
                    return Err(bad("Almacén y carpetas sincronizadas deben ser disjuntos"));
                }
            }
            Ok(())
        }
        fn sync_namespace(
            &self,
            hook: &mut impl FnMut(Barrier) -> io::Result<()>,
        ) -> io::Result<()> {
            self.check()?;
            // Child directory before its parent, including the root namespace.
            // Reopen one prefix at a time: no retained FD per ancestor.
            for depth in (0..self.chain.len()).rev() {
                let (file, _) = Self::walk(&self.parts[..depth], Some(&self.chain))?;
                file.sync_all()?;
                hook(Barrier::DirectorySynced(depth))?;
            }
            self.check()
        }
    }
    struct Held {
        file: File,
        pid: libc::pid_t,
    }
    impl Held {
        fn lock(file: File) -> io::Result<Self> {
            private_file(&file)?;
            loop {
                if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                    break;
                }
                let e = io::Error::last_os_error();
                if e.kind() != io::ErrorKind::Interrupted {
                    return Err(e);
                }
            }
            let held = Self {
                file,
                pid: unsafe { libc::getpid() },
            };
            private_file(&held.file)?;
            Ok(held)
        }
        fn owner(&self) -> io::Result<()> {
            if self.pid != unsafe { libc::getpid() } {
                Err(bad(
                    "Sólo el proceso creador conserva propiedad del almacén",
                ))
            } else {
                Ok(())
            }
        }
    }
    impl Drop for Held {
        fn drop(&mut self) {
            if self.pid == unsafe { libc::getpid() } {
                loop {
                    if unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) } == 0
                        || io::Error::last_os_error().kind() != io::ErrorKind::Interrupted
                    {
                        break;
                    }
                }
            }
        }
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum Barrier {
        Created,
        Write,
        ValuesWritten,
        ReadBack,
        FileSynced,
        DirectorySynced(usize),
        Acknowledged,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum State {
        History,
        Durable,
        Uncertain,
    }
    /// Fresh file/namespace durability check only, never source freshness or
    /// operator approval. Not decodable; reverify before a future effect.
    pub struct BarrierReceipt<'a> {
        store: &'a Store,
        expected: &'a StoredProof,
    }
    impl BarrierReceipt<'_> {
        pub fn verify(&self) -> io::Result<()> {
            self.store.ensure_current()?;
            if !self.expected.matches(self.store.proof()) {
                return Err(bad("Identidad/intención del almacén no coincide"));
            }
            Ok(())
        }
    }
    mod workspace_batch;
    pub use workspace_batch::{MAX_BATCH_METADATA_BYTES, WorkspaceBatchReport};
    mod placements;
    pub use placements::{MAX_PLACEMENT_BYTES, Placement, PlacementRegistry};
    mod index;
    mod journal;
    use index::Index;
    pub use index::ValueRecord;
    pub use journal::{
        CreatedStageNode, CreatedWorkspace, CreationEvidence, Journal, JournalMetadataFailure,
        OwnNamespaceRename, OwnStageMetadata, StageBatchFailure, StageBatchPhase, StageBatchPoint,
        StageBatchReport, StageCreateFailure, StagedBatch, StagingReceipt, StagingWorkspace,
        WorkspaceFailure, WorkspaceLocation, WorkspaceParent,
    };
    pub struct Store {
        directory: Directory,
        held: Held,
        name: CString,
        identity: Node,
        stamp: Stamp,
        proof: Proof,
        state: State,
        index: Index,
    }
    fn name(job: [u8; 16]) -> io::Result<CString> {
        if job == [0; 16] {
            return Err(bad("Job vacío"));
        }
        let mut s = String::from("sync-values-");
        for b in job {
            use std::fmt::Write;
            write!(&mut s, "{b:02x}").expect("string write");
        }
        s.push_str(".record");
        c(s.as_bytes())
    }
    fn check_binding(
        directory: &Directory,
        held: &Held,
        name: &CString,
        identity: Node,
        expected: Option<Stamp>,
    ) -> io::Result<Stamp> {
        held.owner()?;
        directory.check()?;
        private_file(&held.file)?;
        let named = owned(unsafe {
            libc::openat(
                directory.file.as_raw_fd(),
                name.as_ptr(),
                libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })?;
        private_file(&named)?;
        let a = held.file.metadata()?;
        let b = named.metadata()?;
        let current = stamp(&a);
        if node(&held.file)? != identity
            || node(&named)? != identity
            || identity.mount != directory.chain.last().expect("root chain").mount
            || stamp(&b) != current
            || expected.is_some_and(|e| e != current)
        {
            return Err(bad("Archivo de metadatos sustituido/modificado"));
        }
        Ok(current)
    }
    fn proof(
        directory: &Directory,
        identity: Node,
        physical: &Physical<'_, '_>,
        values: &metadata::StoredValues,
        length: u64,
    ) -> io::Result<Proof> {
        let root = directory.chain.last().expect("root chain");
        let mut bytes = [0; 212];
        bytes[..8].copy_from_slice(MAGIC);
        for (offset, n) in [
            (8, root.device),
            (16, root.inode),
            (24, identity.device),
            (32, identity.inode),
            (40, root.mount),
            (48, identity.mount),
            (200, length),
        ] {
            bytes[offset..offset + 8].copy_from_slice(&n.to_le_bytes());
        }
        bytes[56..72].copy_from_slice(&values.job());
        bytes[72..104].copy_from_slice(&physical.plan().record().seal());
        bytes[104..136].copy_from_slice(&physical.plan().seal()?);
        bytes[136..168].copy_from_slice(&physical.seal()?);
        bytes[168..200].copy_from_slice(&values.seal());
        bytes[208..].copy_from_slice(&(values.records() as u32).to_le_bytes());
        let p = Proof { bytes };
        StoredProof::decode(&bytes)?;
        Ok(p)
    }
    fn check_cancel(cancel: &AtomicBool) -> io::Result<()> {
        if cancel.load(Ordering::Acquire) {
            Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Almacén cancelado",
            ))
        } else {
            Ok(())
        }
    }
    struct Sink<'a, F> {
        held: &'a mut Held,
        hook: &'a mut F,
    }
    impl<F: FnMut(Barrier) -> io::Result<()>> Write for Sink<'_, F> {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.held.owner()?;
            private_file(&self.held.file)?;
            (self.hook)(Barrier::Write)?;
            self.held.file.write(b)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.held.file.flush()
        }
    }
    impl Store {
        /// Directory exists already, exact private0700 and outside both inputs.
        /// Failure preserves partial/complete files for explicit inspection.
        pub fn create(
            path: &Path,
            physical: &Physical<'_, '_>,
            prepared: &Prepared,
            job: [u8; 16],
            limit: u64,
            cancel: &AtomicBool,
        ) -> io::Result<Self> {
            Self::create_with(path, physical, prepared, job, limit, cancel, |_| Ok(()))
        }
        pub(super) fn create_with(
            path: &Path,
            physical: &Physical<'_, '_>,
            prepared: &Prepared,
            job: [u8; 16],
            limit: u64,
            cancel: &AtomicBool,
            mut hook: impl FnMut(Barrier) -> io::Result<()>,
        ) -> io::Result<Self> {
            check_cancel(cancel)?;
            let name = name(job)?;
            let records = physical.nodes().len() + 2 * physical.directory_metadata().len();
            if limit > metadata::MAX_STREAM_BYTES
                || limit < 164 + 49 * records as u64
                || prepared.record()?.bytes() != physical.plan().record().bytes()
            {
                return Err(bad("Adquisición de almacén inválida"));
            }
            let directory = Directory::open(path)?;
            directory.disjoint(physical)?;
            directory.check()?;
            let file = owned(unsafe {
                libc::openat(
                    directory.file.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDWR
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_NOFOLLOW
                        | libc::O_NONBLOCK
                        | libc::O_NOATIME
                        | libc::O_CLOEXEC,
                    0o600,
                )
            })?;
            // Only the newly exclusive file; never chmod/truncate an old name.
            if unsafe { libc::fchmod(file.as_raw_fd(), 0o600) } != 0 {
                return Err(io::Error::last_os_error());
            }
            let mut held = Held::lock(file)?;
            let identity = node(&held.file)?;
            check_binding(&directory, &held, &name, identity, None)?;
            hook(Barrier::Created)?;
            let plan_bytes = physical.plan().encode()?;
            let record_bytes = physical.plan().record().bytes();
            let mut header = [0; 24];
            header[..8].copy_from_slice(b"FLSSAV\0\x01");
            header[8..12].copy_from_slice(&(record_bytes.len() as u32).to_le_bytes());
            header[12..16].copy_from_slice(&(plan_bytes.len() as u32).to_le_bytes());
            header[16..24].copy_from_slice(&physical.max_stage_payload().to_le_bytes());
            {
                let mut sink = Sink {
                    held: &mut held,
                    hook: &mut hook,
                };
                sink.write_all(&header)?;
                sink.write_all(record_bytes)?;
                sink.write_all(&plan_bytes)?;
            }
            let prefix_len = 24 + record_bytes.len() as u64 + plan_bytes.len() as u64;
            let acquired = metadata::acquire(
                physical,
                prepared,
                job,
                limit,
                cancel,
                &mut Sink {
                    held: &mut held,
                    hook: &mut hook,
                },
            )?;
            hook(Barrier::ValuesWritten)?;
            let written = check_binding(&directory, &held, &name, identity, None)?;
            if written.length != prefix_len + acquired.bytes() {
                return Err(bad("Longitud no corresponde a adquisición"));
            }
            held.file.seek(SeekFrom::Start(0))?;
            let saved = read_saved(&mut held.file)?;
            if !saved.matches(physical)? {
                return Err(bad("Readback de intención no coincide"));
            }
            let (values, index) =
                Index::collect(physical, &mut held.file, prefix_len, written.length)?;
            if !values.matches(&acquired) {
                return Err(bad("Readback no corresponde a valores emitidos"));
            }
            hook(Barrier::ReadBack)?;
            check_binding(&directory, &held, &name, identity, Some(written))?;
            let proof = proof(&directory, identity, physical, &values, written.length)?;
            let mut store = Self {
                directory,
                held,
                name,
                identity,
                stamp: written,
                proof,
                state: State::Uncertain,
                index,
            };
            store.confirm_with(cancel, &mut hook)?;
            // No callback remains after this revalidation. This still is not a
            // lasting filesystem snapshot or operator approval.
            let _fresh = physical.plan().bind(prepared, cancel)?;
            store.ensure_current()?;
            Ok(store)
        }
        /// Parsed history starts with no current durability barrier.
        pub fn open(path: &Path, physical: &Physical<'_, '_>, job: [u8; 16]) -> io::Result<Self> {
            let (store, saved) = Self::open_saved(path, job)?;
            if !saved.matches(physical)? {
                return Err(bad("Intención guardada distinta del plan esperado"));
            }
            Ok(store)
        }
        /// No original input paths or in-memory Prepared are needed to inspect.
        pub fn open_saved(path: &Path, job: [u8; 16]) -> io::Result<(Self, SavedIntent)> {
            let name = name(job)?;
            let directory = Directory::open(path)?;
            directory.check()?;
            let pin = owned(unsafe {
                libc::openat(
                    directory.file.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            })?;
            private_file(&pin)?;
            let identity = node(&pin)?;
            let fd_path = c(format!("/proc/self/fd/{}", pin.as_raw_fd()).as_bytes())?;
            let file = owned(unsafe {
                libc::open(
                    fd_path.as_ptr(),
                    libc::O_RDWR | libc::O_NONBLOCK | libc::O_NOATIME | libc::O_CLOEXEC,
                )
            })?;
            let mut held = Held::lock(file)?;
            let observed = check_binding(&directory, &held, &name, identity, None)?;
            if observed.length > MAX_ARTIFACT {
                return Err(bad("Registro excesivo"));
            }
            let saved = read_saved(&mut held.file)?;
            let (proof, index) = {
                let plan = saved.plan()?;
                let physical = Physical::derive_with_limit(&plan, saved.stage_limit)?;
                directory.disjoint(&physical)?;
                let (values, index) = Index::collect(
                    &physical,
                    &mut held.file,
                    saved.prefix_len(),
                    observed.length,
                )?;
                if values.job() != job || values.bytes() + saved.prefix_len() != observed.length {
                    return Err(bad("Job/longitud de registro incorrecto"));
                }
                (
                    proof(&directory, identity, &physical, &values, observed.length)?,
                    index,
                )
            };
            check_binding(&directory, &held, &name, identity, Some(observed))?;
            Ok((
                Self {
                    directory,
                    held,
                    name,
                    identity,
                    stamp: observed,
                    proof,
                    state: State::History,
                    index,
                },
                saved,
            ))
        }
        pub fn state(&self) -> State {
            self.state
        }
        pub fn proof(&self) -> &Proof {
            &self.proof
        }
        pub fn path(&self) -> PathBuf {
            self.directory
                .path
                .join(OsString::from_vec(self.name.as_bytes().to_vec()))
        }
        fn binding(&self) -> io::Result<()> {
            check_binding(
                &self.directory,
                &self.held,
                &self.name,
                self.identity,
                Some(self.stamp),
            )?;
            Ok(())
        }
        pub fn ensure_current(&self) -> io::Result<()> {
            if self.state != State::Durable {
                return Err(bad(
                    "Sin confirmación durable vigente; reabrir si ACK incierto",
                ));
            }
            self.binding()
        }
        pub fn barrier<'a>(&'a self, expected: &'a StoredProof) -> io::Result<BarrierReceipt<'a>> {
            let receipt = BarrierReceipt {
                store: self,
                expected,
            };
            receipt.verify()?;
            Ok(receipt)
        }
        pub fn require_proof(&self, expected: &StoredProof) -> io::Result<()> {
            self.binding()?;
            if !expected.matches(&self.proof) {
                return Err(bad("Identidad/intención del almacén no coincide"));
            }
            Ok(())
        }
        pub fn inspect(
            &mut self,
            physical: &Physical<'_, '_>,
            visit: impl FnMut(Target, bool, Values) -> io::Result<()>,
        ) -> io::Result<()> {
            self.binding()?;
            self.held.file.seek(SeekFrom::Start(0))?;
            let saved = read_saved(&mut self.held.file)?;
            if !saved.matches(physical)? {
                return Err(bad("Intención guardada no coincide"));
            }
            let values = metadata::inspect(physical, &mut self.held.file, visit)?;
            if proof(
                &self.directory,
                self.identity,
                physical,
                &values,
                self.stamp.length,
            )? != self.proof
            {
                return Err(bad("Contenido/intención del almacén cambió"));
            }
            self.binding()
        }
        /// Read one original validated record. History/Uncertain reads are
        /// diagnostic data, not an Apply gate. No shared file cursor is changed.
        pub fn value(&self, expected: &StoredProof, target: Target) -> io::Result<ValueRecord> {
            self.value_with(expected, target, || Ok(()))
        }
        fn value_with(
            &self,
            expected: &StoredProof,
            target: Target,
            after_read: impl FnOnce() -> io::Result<()>,
        ) -> io::Result<ValueRecord> {
            self.require_proof(expected)?;
            let value = self.index.read(&self.held.file, target)?;
            after_read()?;
            self.binding()?;
            Ok(value)
        }
        pub fn renew(
            &mut self,
            physical: &Physical<'_, '_>,
            cancel: &AtomicBool,
        ) -> io::Result<()> {
            self.renew_with(physical, cancel, |_| Ok(()))
        }
        pub(super) fn renew_with(
            &mut self,
            physical: &Physical<'_, '_>,
            cancel: &AtomicBool,
            mut hook: impl FnMut(Barrier) -> io::Result<()>,
        ) -> io::Result<()> {
            if self.state == State::Uncertain {
                return Err(bad("ACK incierto: esta instancia no puede renovar"));
            }
            self.held.owner()?;
            self.state = State::Uncertain;
            self.binding()?;
            check_cancel(cancel)?;
            self.inspect(physical, |_, _, _| Ok(()))?;
            hook(Barrier::ReadBack)?;
            self.confirm_with(cancel, &mut hook)
        }
        fn confirm_with(
            &mut self,
            cancel: &AtomicBool,
            hook: &mut impl FnMut(Barrier) -> io::Result<()>,
        ) -> io::Result<()> {
            self.binding()?;
            check_cancel(cancel)?;
            self.held.file.sync_all()?;
            hook(Barrier::FileSynced)?;
            self.binding()?;
            self.directory.sync_namespace(hook)?;
            check_cancel(cancel)?;
            hook(Barrier::Acknowledged)?;
            self.binding()?;
            check_cancel(cancel)?;
            self.state = State::Durable;
            Ok(())
        }
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::{
            directory_sync::Mode,
            directory_sync_manifest::capture,
            directory_sync_plan::{Options, Plan},
            directory_sync_wire::Intent,
        };
        use std::{
            fs,
            io::Read,
            os::unix::fs::{PermissionsExt, symlink},
            process::Command,
            sync::atomic::{AtomicU64, Ordering},
        };
        struct Fixture {
            base: PathBuf,
            paths: [PathBuf; 2],
            store: PathBuf,
        }
        impl Fixture {
            fn new() -> Self {
                static N: AtomicU64 = AtomicU64::new(0);
                let base = std::env::temp_dir().join(format!(
                    "files-sync-store-{}-{}",
                    std::process::id(),
                    N.fetch_add(1, Ordering::Relaxed)
                ));
                let paths = [base.join("a"), base.join("b")];
                let store = base.join("values");
                for p in [&paths[0], &paths[1], &store] {
                    fs::create_dir_all(p).unwrap();
                }
                fs::set_permissions(&store, fs::Permissions::from_mode(0o700)).unwrap();
                fs::write(paths[0].join("x"), b"source").unwrap();
                Self { base, paths, store }
            }
            fn prepare(&self) -> Prepared {
                capture(
                    Intent {
                        version: 1,
                        id: 31,
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
            fn path(&self, job: [u8; 16]) -> PathBuf {
                self.store
                    .join(OsString::from_vec(name(job).unwrap().as_bytes().to_vec()))
            }
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.base);
            }
        }
        fn create(f: &Fixture, p: &Physical<'_, '_>, prepared: &Prepared, job: [u8; 16]) -> Store {
            Store::create(
                &f.store,
                p,
                prepared,
                job,
                metadata::DEFAULT_STREAM_BYTES,
                &AtomicBool::new(false),
            )
            .unwrap()
        }
        #[test]
        fn admission_oversized_remapped_workspace_rejects_before_journal_but_history_opens() {
            let f = Fixture::new();
            fs::remove_file(f.paths[0].join("x")).unwrap();
            fs::create_dir(f.paths[0].join("x")).unwrap();
            let mut parent = File::open(f.paths[0].join("x")).unwrap();
            let component = "d".repeat(255);
            for _ in 0..15 {
                let next =
                    PathBuf::from(format!("/proc/self/fd/{}", parent.as_raw_fd())).join(&component);
                fs::create_dir(&next).unwrap();
                parent = File::open(next).unwrap();
            }
            let leaf = PathBuf::from(format!("/proc/self/fd/{}", parent.as_raw_fd()))
                .join("f".repeat(254));
            fs::write(leaf, b"payload").unwrap();
            drop(parent);
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            assert!(physical.staging_admission().is_err());
            let store = create(&f, &physical, &prepared, [78; 16]);
            let before = fs::read_dir(&f.store).unwrap().count();
            assert!(Journal::create(&store, &physical, &AtomicBool::new(false)).is_err());
            assert_eq!(fs::read_dir(&f.store).unwrap().count(), before);
            // Historical sealed data remains inspectable even when new execution
            // admission rejects its geometry. Construct a legacy empty header.
            let schema = crate::directory_sync_journal::Schema::derive(&store, &physical).unwrap();
            let journal_path = f
                .store
                .join("sync-effects-4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e4e.journal");
            fs::write(
                &journal_path,
                crate::directory_sync_journal::header(&schema),
            )
            .unwrap();
            fs::set_permissions(&journal_path, fs::Permissions::from_mode(0o600)).unwrap();
            let mut journal = Journal::open(&store, &physical).unwrap();
            assert!(journal.renew(&AtomicBool::new(false)).is_ok());
            assert!(schema.staging_admission().is_err());
        }
        fn factory_parent(f: &Fixture) -> PathBuf {
            let p = f.base.join("factory");
            fs::create_dir(&p).unwrap();
            fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
            p
        }
        fn factory_journal<'a>(store: &'a Store, physical: &Physical<'_, '_>) -> Journal<'a> {
            let cancel = AtomicBool::new(false);
            let mut j = Journal::create(store, physical, &cancel).unwrap();
            j.append(
                crate::directory_sync_journal::Event::Approval { ticket: [8; 16] },
                &cancel,
            )
            .unwrap();
            j
        }
        #[test]
        fn workspace_factory_complete_creator_metadata_barrier_and_publication() {
            use crate::{
                directory_sync_journal::{Event, NamespaceAction, Object, Observation},
                exdev_source::{NamespacePath, NamespaceScope, plan_namespace_move},
            };
            let f = Fixture::new();
            let container = factory_parent(&f);
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [131; 16]);
            let mut j = factory_journal(&store, &physical);
            let flag = AtomicBool::new(false);
            let result = j
                .create_workspaces(&physical, &[&container], &flag)
                .unwrap()
                .pop()
                .unwrap();
            assert_eq!(result.transaction(), 0);
            assert_ne!(result.seal(), [0; 32]);
            assert!(
                result.fd_slots_observed()
                    >= physical.staging_admission().unwrap().retained_fd_slots()
            );
            assert_eq!(result.root().metadata().unwrap().mode() & 0o7777, 0o700);
            let (_, parent, name) = result.into_parts();
            let source = prepared.namespace_snapshot(0, 1024, &flag).unwrap();
            let root = File::open(container.join(&name)).unwrap();
            let receipt = j
                .create_stage_node(0, &source, &root, None, 1024, &flag)
                .unwrap();
            drop(receipt);
            drop(root);
            let mut inventory =
                crate::exdev_source::capture_namespace(&parent, &name, 1024, &flag).unwrap();
            j.apply_stage_metadata(
                0,
                vec![NamespaceScope {
                    inventory: &mut inventory,
                    parent: &parent,
                    name: &name,
                }],
                1024,
                &flag,
            )
            .unwrap();
            let e = &inventory.entries()[1];
            j.append(
                Event::NodeVerified {
                    node: 0,
                    observed: Observation {
                        object: Object {
                            identity: e.identity,
                            mount: inventory.mounts().unwrap()[1],
                        },
                        revision: e.revision,
                        content: e.content_digest,
                        metadata: e.metadata_digest,
                    },
                },
                &flag,
            )
            .unwrap();
            j.confirm_staging(
                &[StagingWorkspace {
                    inventory: &inventory,
                    parent: &parent,
                    name: &name,
                }],
                1024,
                &flag,
            )
            .unwrap();
            for event in [
                Event::EffectIntent(0),
                Event::AbsenceChecked(0),
                Event::PublishIntent(0),
            ] {
                j.append(event, &flag).unwrap();
            }
            let mut input = prepared.namespace_snapshot(1, 1024, &flag).unwrap();
            let rename = plan_namespace_move(
                vec![
                    NamespaceScope {
                        inventory: &mut inventory,
                        parent: &parent,
                        name: &name,
                    },
                    NamespaceScope {
                        inventory: &mut input.inventory,
                        parent: &input.parent,
                        name: &input.name,
                    },
                ],
                NamespacePath {
                    scope: 0,
                    relative: b"new",
                },
                NamespacePath {
                    scope: 1,
                    relative: b"x",
                },
                1024,
                &flag,
            )
            .unwrap();
            j.execute_namespace(0, NamespaceAction::Publish, rename, 1024, &flag)
                .unwrap();
            assert_eq!(fs::read(f.paths[1].join("x")).unwrap(), b"source");
            assert!(j.history().state.workspace_create_guard().is_none());
            assert!(
                j.create_workspaces(&physical, &[&container], &flag)
                    .is_err()
            );
        }
        #[test]
        fn workspace_factory_all_placements_preflight_before_any_artifact_or_intent() {
            for invalid in 0..6 {
                let f = Fixture::new();
                let container = factory_parent(&f);
                fs::write(f.paths[0].join("y"), b"two").unwrap();
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [132; 16]);
                let mut j = factory_journal(&store, &physical);
                let bad_path = match invalid {
                    0 => f.paths[0].clone(),
                    1 => f.base.clone(),
                    2 => f.store.clone(),
                    3 => {
                        let p = f.base.join("public");
                        fs::create_dir(&p).unwrap();
                        p
                    }
                    4 => {
                        let p = f.base.join("alias");
                        symlink(&container, &p).unwrap();
                        p
                    }
                    _ => PathBuf::from("relative"),
                };
                let length = fs::metadata(j.path()).unwrap().len();
                let err = j
                    .create_workspaces(&physical, &[&container, &bad_path], &AtomicBool::new(false))
                    .unwrap_err();
                assert!(!err.intent_acknowledged && !err.syscall_attempted);
                assert_eq!(fs::metadata(j.path()).unwrap().len(), length);
                assert_eq!(fs::read_dir(&container).unwrap().count(), 0);
                j.ensure_current().unwrap();
            }
        }
        #[test]
        fn workspace_factory_faults_preserve_partials_and_pending_history_blocks_retry() {
            use journal::WorkspacePoint;
            for point in 0..12 {
                let f = Fixture::new();
                let container = factory_parent(&f);
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [133; 16]);
                let mut j = factory_journal(&store, &physical);
                let err = j
                    .create_workspaces_with(
                        &physical,
                        &[&container],
                        &AtomicBool::new(false),
                        |at| {
                            let n = match at {
                                WorkspacePoint::Admitted => 0,
                                WorkspacePoint::IntentWritten => 1,
                                WorkspacePoint::IntentFileSynced => 2,
                                WorkspacePoint::IntentAcknowledged => 3,
                                WorkspacePoint::IntentDurable => 4,
                                WorkspacePoint::BeforeCreate => 5,
                                WorkspacePoint::Created => 6,
                                WorkspacePoint::IdentityRecorded => 7,
                                WorkspacePoint::RootSynced => 8,
                                WorkspacePoint::NamespaceSynced => 9,
                                WorkspacePoint::MaterializedAcknowledged => 10,
                                WorkspacePoint::Ready => 11,
                            };
                            if n == point {
                                Err(io::Error::other("factory fault"))
                            } else {
                                Ok(())
                            }
                        },
                    )
                    .unwrap_err();
                assert_eq!(err.intent_acknowledged, point >= 4);
                assert_eq!(err.syscall_attempted, point >= 6);
                assert_eq!(err.syscall_succeeded, point >= 6);
                assert_eq!(err.identity_recorded, point >= 7);
                assert_eq!(err.materialized_acknowledged, point >= 10);
                assert_eq!(
                    fs::read_dir(&container).unwrap().count(),
                    usize::from(point >= 6)
                );
                assert_eq!(fs::read(f.paths[0].join("x")).unwrap(), b"source");
                if point == 0 {
                    j.ensure_current().unwrap();
                    continue;
                }
                assert_eq!(j.durability(), State::Uncertain);
                drop(j);
                let mut reopened = Journal::open(&store, &physical).unwrap();
                assert_eq!(reopened.durability(), State::History);
                reopened.renew(&AtomicBool::new(false)).unwrap();
                assert!(
                    reopened
                        .create_workspaces(&physical, &[&container], &AtomicBool::new(false))
                        .is_err()
                );
            }
        }
        #[test]
        fn workspace_factory_batch_keeps_completed_receipts_and_rejects_collisions() {
            use journal::WorkspacePoint;
            for collision in [false, true] {
                let f = Fixture::new();
                let container = factory_parent(&f);
                fs::write(f.paths[0].join("y"), b"two").unwrap();
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [134; 16]);
                let mut j = factory_journal(&store, &physical);
                let mut calls = 0;
                let err = j
                    .create_workspaces_with(
                        &physical,
                        &[&container, &container],
                        &AtomicBool::new(false),
                        |at| {
                            if matches!(at, WorkspacePoint::BeforeCreate) {
                                calls += 1;
                                if calls == 2 {
                                    if collision {
                                        fs::write(
                                            container.join(
                                                "sync-86868686868686868686868686868686-00000001",
                                            ),
                                            b"do not replace",
                                        )?;
                                    } else {
                                        return Err(io::Error::other("second workspace"));
                                    }
                                }
                            }
                            Ok(())
                        },
                    )
                    .unwrap_err();
                assert_eq!(err.completed.len(), 1);
                assert!(err.intent_acknowledged);
                assert_eq!(err.syscall_attempted, collision);
                assert!(!err.syscall_succeeded);
                assert_eq!(j.durability(), State::Uncertain);
                if collision {
                    assert_eq!(fs::read(err.path.unwrap()).unwrap(), b"do not replace");
                }
                assert_eq!(
                    fs::read_dir(&container).unwrap().count(),
                    1 + usize::from(collision)
                );
            }
        }
        #[test]
        fn workspace_factory_raw_parent_multiple_roots_and_strict_guard_codec() {
            use crate::directory_sync_journal::{self as wire, Event, Object};
            let f = Fixture::new();
            let original = factory_parent(&f);
            let container = f.base.join(OsString::from_vec(b"raw-\xff\n".to_vec()));
            fs::rename(original, &container).unwrap();
            fs::write(f.paths[0].join("y"), b"two").unwrap();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [135; 16]);
            let mut j = factory_journal(&store, &physical);
            let workspaces = j
                .create_workspaces(
                    &physical,
                    &[&container, &container],
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(workspaces.len(), 2);
            assert_ne!(
                workspaces[0].root().metadata().unwrap().ino(),
                workspaces[1].root().metadata().unwrap().ino()
            );
            assert_eq!(fs::read_dir(&container).unwrap().count(), 2);
            let schema = wire::Schema::derive(&store, &physical).unwrap();
            let mut state = wire::State::new(&schema);
            state
                .accept(&schema, Event::Approval { ticket: [7; 16] })
                .unwrap();
            assert!(
                state
                    .accept(&schema, Event::PlacementRegistered { seal: [0; 32] })
                    .is_err()
            );
            let registered = Event::PlacementRegistered { seal: [9; 32] };
            let mut registered_bytes = wire::encode_event(registered);
            assert_eq!(registered_bytes.len(), 33);
            assert_eq!(wire::decode_event(&registered_bytes).unwrap(), registered);
            registered_bytes.push(0);
            assert!(wire::decode_event(&registered_bytes).is_err());
            state.accept(&schema, registered).unwrap();
            assert!(state.accept(&schema, registered).is_err());
            let intent = Event::WorkspaceCreateIntent {
                transaction: 0,
                seal: [5; 32],
            };
            let bytes = wire::encode_event(intent);
            assert_eq!(bytes.len(), 37);
            assert_eq!(wire::decode_event(&bytes).unwrap(), intent);
            let mut extra = bytes;
            extra.push(0);
            assert!(wire::decode_event(&extra).is_err());
            assert!(
                state
                    .accept(
                        &schema,
                        Event::WorkspaceCreateIntent {
                            transaction: 0,
                            seal: [0; 32]
                        }
                    )
                    .is_err()
            );
            state.accept(&schema, intent).unwrap();
            assert!(state.accept(&schema, Event::WorkspaceIntent(0)).is_err());
            let root = workspaces[0].root();
            state
                .accept(
                    &schema,
                    Event::WorkspaceCreated {
                        transaction: 0,
                        object: Object {
                            identity: crate::exdev_source::observed_identity(
                                &root.metadata().unwrap(),
                            )
                            .unwrap(),
                            mount: node(root).unwrap().mount,
                        },
                    },
                )
                .unwrap();
            assert!(state.accept(&schema, Event::WorkspaceIntent(1)).is_err());
            assert!(
                state
                    .accept(&schema, Event::WorkspaceMaterialized(1))
                    .is_err()
            );
            let materialized = Event::WorkspaceMaterialized(0);
            assert_eq!(wire::encode_event(materialized).len(), 5);
            assert_eq!(
                wire::decode_event(&wire::encode_event(materialized)).unwrap(),
                materialized
            );
            state.accept(&schema, materialized).unwrap();
            state.accept(&schema, Event::WorkspaceIntent(1)).unwrap();
        }
        #[test]
        fn workspace_factory_cooperative_pause_resume_and_cancel_precede_effects() {
            use crate::copy_engine::Control;
            use std::{sync::mpsc, time::Duration};
            for cancelled in [false, true] {
                let f = Fixture::new();
                let container = factory_parent(&f);
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [136; 16]);
                let mut j = factory_journal(&store, &physical);
                let control = Control::default();
                control.pause();
                let worker_control = control.clone();
                std::thread::scope(|scope| {
                    let (enter_tx, enter_rx) = mpsc::channel();
                    let (done_tx, done_rx) = mpsc::channel();
                    let owner = &mut j;
                    let p = &physical;
                    let path = &container;
                    let worker = scope.spawn(move || {
                        enter_tx.send(()).unwrap();
                        done_tx
                            .send(owner.create_workspaces_controlled(p, &[path], &worker_control))
                            .unwrap();
                    });
                    enter_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                    assert!(done_rx.recv_timeout(Duration::from_millis(30)).is_err());
                    assert_eq!(fs::read_dir(&container).unwrap().count(), 0);
                    if cancelled {
                        control.cancel();
                    } else {
                        control.resume();
                    }
                    let result = done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                    worker.join().unwrap();
                    if cancelled {
                        let err = result.unwrap_err();
                        assert!(!err.syscall_attempted && !err.intent_acknowledged);
                    } else {
                        assert_eq!(result.unwrap().len(), 1);
                    }
                });
                j.ensure_current().unwrap();
            }
        }
        #[test]
        fn workspace_factory_rejects_changed_journal_and_post_ack_replacement() {
            use journal::WorkspacePoint;
            for replacement in [false, true] {
                let f = Fixture::new();
                let container = factory_parent(&f);
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [137; 16]);
                let mut j = factory_journal(&store, &physical);
                let journal_path = j.path();
                let path = container.join("sync-89898989898989898989898989898989-00000000");
                let retained = container.join("retained");
                let err = j
                    .create_workspaces_with(
                        &physical,
                        &[&container],
                        &AtomicBool::new(false),
                        |at| {
                            if replacement && matches!(at, WorkspacePoint::MaterializedAcknowledged)
                            {
                                fs::rename(&path, &retained)?;
                                fs::create_dir(&path)?;
                                fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
                            } else if !replacement && matches!(at, WorkspacePoint::BeforeCreate) {
                                let bytes = fs::read(&journal_path)?;
                                fs::write(&journal_path, bytes)?;
                            }
                            Ok(())
                        },
                    )
                    .unwrap_err();
                assert_eq!(err.syscall_succeeded, replacement);
                assert_eq!(err.materialized_acknowledged, replacement);
                assert_eq!(err.completed.len(), usize::from(replacement));
                assert_eq!(j.durability(), State::Uncertain);
                assert_eq!(
                    fs::read_dir(&container).unwrap().count(),
                    if replacement { 2 } else { 0 }
                );
            }
        }
        #[test]
        fn workspace_factory_actual_fd_limit_and_umask_fail_closed_in_child() {
            const ENV: &str = "FILES_FACTORY_LIMIT_CHILD";
            if let Ok(mode) = std::env::var(ENV) {
                let f = Fixture::new();
                let container = factory_parent(&f);
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [138; 16]);
                let mut j = factory_journal(&store, &physical);
                if mode == "fd" {
                    let mut original = std::mem::MaybeUninit::<libc::rlimit>::zeroed();
                    assert_eq!(
                        unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, original.as_mut_ptr()) },
                        0
                    );
                    let original = unsafe { original.assume_init() };
                    let low = libc::rlimit {
                        rlim_cur: 64,
                        rlim_max: original.rlim_max,
                    };
                    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &low) }, 0);
                    let result =
                        j.create_workspaces(&physical, &[&container], &AtomicBool::new(false));
                    assert_eq!(
                        unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &original) },
                        0
                    );
                    let err = result.unwrap_err();
                    assert!(!err.syscall_attempted && !err.intent_acknowledged);
                    j.ensure_current().unwrap();
                    assert_eq!(fs::read_dir(&container).unwrap().count(), 0);
                } else {
                    let previous = unsafe { libc::umask(0o777) };
                    let result =
                        j.create_workspaces(&physical, &[&container], &AtomicBool::new(false));
                    unsafe {
                        libc::umask(previous);
                    }
                    let err = result.unwrap_err();
                    assert!(
                        err.syscall_attempted && err.syscall_succeeded && !err.identity_recorded
                    );
                    assert_eq!(j.durability(), State::Uncertain);
                    let path = fs::read_dir(&container)
                        .unwrap()
                        .next()
                        .unwrap()
                        .unwrap()
                        .path();
                    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o7777, 0);
                    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
                }
                return;
            }
            for mode in ["fd", "umask"] {
                let status = Command::new(std::env::current_exe().unwrap()).args(["--exact", "directory_sync_store::linux::tests::workspace_factory_actual_fd_limit_and_umask_fail_closed_in_child", "--nocapture"]).env(ENV, mode).status().unwrap();
                assert!(status.success());
            }
        }
        #[test]
        fn workspace_factory_real_exits_reopen_history_without_implicit_recreation() {
            use journal::WorkspacePoint;
            const ENV: &str = "FILES_FACTORY_EXIT_BASE";
            if let Some(base) = std::env::var_os(ENV) {
                let base = PathBuf::from(base);
                let f = Fixture {
                    paths: [base.join("a"), base.join("b")],
                    store: base.join("values"),
                    base,
                };
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let mut store = Store::open(&f.store, &physical, [139; 16]).unwrap();
                let flag = AtomicBool::new(false);
                // Reopen remains History until both artifact owners are renewed.
                store.renew(&physical, &flag).unwrap();
                let mut j = Journal::open(&store, &physical).unwrap();
                j.renew(&flag).unwrap();
                let point = std::env::var("FILES_FACTORY_EXIT_POINT").unwrap();
                j.create_workspaces_with(&physical, &[&f.base.join("factory")], &flag, |at| {
                    if matches!(
                        (point.as_str(), at),
                        ("intent", WorkspacePoint::IntentDurable)
                            | ("created", WorkspacePoint::Created)
                            | ("confirmed", WorkspacePoint::MaterializedAcknowledged)
                    ) {
                        unsafe { libc::_exit(103) }
                    }
                    Ok(())
                })
                .unwrap();
                panic!("exit hook missing");
            }
            for point in ["intent", "created", "confirmed"] {
                let f = Fixture::new();
                let container = factory_parent(&f);
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [139; 16]);
                let j = factory_journal(&store, &physical);
                drop(j);
                drop(store);
                let status = Command::new(std::env::current_exe().unwrap()).args(["--exact", "directory_sync_store::linux::tests::workspace_factory_real_exits_reopen_history_without_implicit_recreation", "--nocapture"]).env(ENV, &f.base).env("FILES_FACTORY_EXIT_POINT", point).status().unwrap();
                assert_eq!(status.code(), Some(103));
                assert_eq!(
                    fs::read_dir(&container).unwrap().count(),
                    usize::from(point != "intent")
                );
                let store = Store::open(&f.store, &physical, [139; 16]).unwrap();
                let j = Journal::open(&store, &physical).unwrap();
                assert_eq!(j.durability(), State::History);
                assert_eq!(
                    j.history().state.workspace_create_guard().is_some(),
                    point != "confirmed"
                );
                let registry = store
                    .open_placements(&physical, &AtomicBool::new(false))
                    .unwrap();
                assert_eq!(registry.durability(), State::History);
                assert_eq!(registry.entry(0).unwrap().parent_path(), container);
                assert_eq!(j.history().state.placement_seal(), Some(registry.seal()));
                assert!(j.ensure_current().is_err());
            }
        }
        #[test]
        fn workspace_factory_foreign_mount_rejects_before_first_workspace() {
            let f = Fixture::new();
            let container = factory_parent(&f);
            fs::write(f.paths[0].join("y"), b"two").unwrap();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            struct Foreign(PathBuf);
            impl Drop for Foreign {
                fn drop(&mut self) {
                    let _ = fs::remove_dir(&self.0);
                }
            }
            let foreign = Foreign(PathBuf::from("/dev/shm").join(f.base.file_name().unwrap()));
            fs::create_dir(&foreign.0).unwrap();
            fs::set_permissions(&foreign.0, fs::Permissions::from_mode(0o700)).unwrap();
            assert_ne!(
                node(&File::open(&foreign.0).unwrap()).unwrap().mount,
                physical.transactions()[1].mount()
            );
            let store = create(&f, &physical, &prepared, [140; 16]);
            let mut j = factory_journal(&store, &physical);
            let length = fs::metadata(j.path()).unwrap().len();
            let err = j
                .create_workspaces(
                    &physical,
                    &[&container, &foreign.0],
                    &AtomicBool::new(false),
                )
                .unwrap_err();
            assert!(!err.intent_acknowledged && !err.syscall_attempted);
            assert_eq!(fs::metadata(j.path()).unwrap().len(), length);
            assert_eq!(fs::read_dir(&container).unwrap().count(), 0);
            assert_eq!(fs::read_dir(&foreign.0).unwrap().count(), 0);
            j.ensure_current().unwrap();
        }
        fn registry_inputs(
            schema: &crate::directory_sync_journal::Schema,
            paths: &[&Path],
        ) -> Vec<workspace_batch::PlacementSlot> {
            paths
                .iter()
                .enumerate()
                .map(|(i, path)| {
                    let directory = Directory::open(path).unwrap();
                    let name = placements::workspace_name(schema.proof(), i);
                    let n = node(&directory.file).unwrap();
                    let object = crate::directory_sync_journal::Object {
                        identity: crate::move_manifest::Identity {
                            device: n.device,
                            inode: n.inode,
                            kind: Kind::Directory,
                        },
                        mount: n.mount,
                    };
                    let seal = schema
                        .workspace_binding(i as u32, object, name.as_bytes())
                        .unwrap();
                    (std::sync::Arc::new(directory), name, seal)
                })
                .collect()
        }
        #[test]
        fn placement_registry_history_raw_locations_renewal_and_explicit_journal_attachment() {
            let f = Fixture::new();
            let original = factory_parent(&f);
            let path = f
                .base
                .join(OsString::from_vec(b"placements-\xff\n".to_vec()));
            fs::rename(original, &path).unwrap();
            fs::write(f.paths[0].join("y"), b"two").unwrap();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [141; 16]);
            let mut j = factory_journal(&store, &physical);
            let flag = AtomicBool::new(false);
            let roots = j
                .create_workspaces(&physical, &[&path, &path], &flag)
                .unwrap();
            let live = j.placements().unwrap();
            assert_eq!(live.durability(), State::Durable);
            assert_eq!(live.len(), 2);
            assert_eq!(j.history().state.placement_seal(), Some(live.seal()));
            for (i, root) in roots.iter().enumerate() {
                let entry = live.entry(i).unwrap();
                assert_eq!(entry.transaction(), i);
                assert_eq!(entry.parent_path(), path);
                assert_eq!(entry.name(), root.name());
                assert_eq!(entry.seal(), root.seal());
                assert_eq!(
                    node(&entry.open_parent().unwrap()).unwrap(),
                    node(root.parent()).unwrap()
                );
                assert!(entry.ancestors().count() > 1);
            }
            assert!(live.entry(2).is_err());
            drop(roots);
            drop(j);
            let mut j = Journal::open(&store, &physical).unwrap();
            j.renew(&flag).unwrap();
            assert!(j.ensure_current().is_err());
            let registry = store.open_placements(&physical, &flag).unwrap();
            assert_eq!(registry.durability(), State::History);
            assert!(registry.ensure_current().is_err());
            assert!(j.attach_placements(registry).is_err());
            let mut registry = store.open_placements(&physical, &flag).unwrap();
            registry.renew(&physical, &flag).unwrap();
            j.attach_placements(registry).unwrap();
            j.ensure_current().unwrap();
            let retained = f.base.join("relocated");
            fs::rename(&path, &retained).unwrap();
            let entry = j.placements().unwrap().entry(0).unwrap();
            assert_eq!(entry.parent_path(), path);
            assert!(entry.open_parent().is_err());
            // Artifact barriers do not assert current placement paths or native approval.
            j.ensure_current().unwrap();
            drop(j);
            let history = store.open_placements(&physical, &flag).unwrap();
            assert_eq!(history.entry(0).unwrap().parent_path(), path);
        }
        #[test]
        fn placement_registry_fault_boundaries_preserve_complete_or_partial_history() {
            for point in 0..8 {
                let f = Fixture::new();
                let path = factory_parent(&f);
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [142; 16]);
                let schema =
                    crate::directory_sync_journal::Schema::derive(&store, &physical).unwrap();
                let inputs = registry_inputs(&schema, &[&path]);
                let mut writes = 0;
                let result = PlacementRegistry::create_with(
                    &store,
                    &schema,
                    &inputs,
                    &AtomicBool::new(false),
                    |at| {
                        let fail = match at {
                            Barrier::Created => point == 0,
                            Barrier::Write => {
                                writes += 1;
                                (point == 1 && writes == 1) || (point == 2 && writes == 2)
                            }
                            Barrier::ValuesWritten => point == 3,
                            Barrier::ReadBack => point == 4,
                            Barrier::FileSynced => point == 5,
                            Barrier::DirectorySynced(0) => point == 6,
                            Barrier::Acknowledged => point == 7,
                            _ => false,
                        };
                        if fail {
                            Err(io::Error::other("placement fault"))
                        } else {
                            Ok(())
                        }
                    },
                );
                assert!(result.is_err());
                assert_eq!(fs::read_dir(&path).unwrap().count(), 0);
                let reopened = store.open_placements(&physical, &AtomicBool::new(false));
                if point < 3 {
                    assert!(reopened.is_err());
                } else {
                    let data = reopened.unwrap();
                    assert_eq!(data.durability(), State::History);
                    assert_eq!(data.entry(0).unwrap().parent_path(), path);
                }
                assert!(
                    PlacementRegistry::create_with(
                        &store,
                        &schema,
                        &inputs,
                        &AtomicBool::new(false),
                        |_| Ok(())
                    )
                    .is_err()
                );
                assert_eq!(fs::read(f.paths[0].join("x")).unwrap(), b"source");
            }
        }
        #[test]
        fn placement_registry_same_byte_rewrite_owner_and_renewal_ack_are_strict() {
            for renewal in [false, true] {
                let f = Fixture::new();
                let path = factory_parent(&f);
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [143; 16]);
                let schema =
                    crate::directory_sync_journal::Schema::derive(&store, &physical).unwrap();
                let inputs = registry_inputs(&schema, &[&path]);
                let filename = f
                    .store
                    .join("sync-placements-8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f8f.registry");
                if !renewal {
                    let result = PlacementRegistry::create_with(
                        &store,
                        &schema,
                        &inputs,
                        &AtomicBool::new(false),
                        |at| {
                            if at == Barrier::ValuesWritten {
                                let bytes = fs::read(&filename)?;
                                fs::write(&filename, bytes)?;
                            }
                            Ok(())
                        },
                    );
                    assert!(result.is_err());
                } else {
                    drop(
                        PlacementRegistry::create_with(
                            &store,
                            &schema,
                            &inputs,
                            &AtomicBool::new(false),
                            |_| Ok(()),
                        )
                        .unwrap(),
                    );
                    let mut registry = store
                        .open_placements(&physical, &AtomicBool::new(false))
                        .unwrap();
                    assert!(
                        registry
                            .renew_with(&schema, &AtomicBool::new(false), &mut |at| if at
                                == Barrier::Acknowledged
                            {
                                Err(io::Error::other("ack"))
                            } else {
                                Ok(())
                            })
                            .is_err()
                    );
                    assert_eq!(registry.durability(), State::Uncertain);
                    assert!(registry.renew(&physical, &AtomicBool::new(false)).is_err());
                    assert!(registry.entry(0).is_err());
                }
            }
        }
        #[test]
        fn placement_registry_tampering_before_mkdir_blocks_registered_factory() {
            use journal::WorkspacePoint;
            let f = Fixture::new();
            let path = factory_parent(&f);
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [144; 16]);
            let mut j = factory_journal(&store, &physical);
            let filename = f
                .store
                .join("sync-placements-90909090909090909090909090909090.registry");
            let err = j
                .create_workspaces_with(&physical, &[&path], &AtomicBool::new(false), |at| {
                    if matches!(at, WorkspacePoint::IntentDurable) {
                        let bytes = fs::read(&filename)?;
                        fs::write(&filename, bytes)?;
                    }
                    Ok(())
                })
                .unwrap_err();
            assert!(err.intent_acknowledged);
            assert!(!err.syscall_attempted);
            assert_eq!(j.durability(), State::Uncertain);
            assert_eq!(fs::read_dir(&path).unwrap().count(), 0);
            drop(j);
            let history = Journal::open(&store, &physical).unwrap();
            assert!(history.history().state.placement_seal().is_some());
            assert!(history.history().state.workspace_create_guard().is_some());
        }
        #[test]
        fn placement_registry_forged_history_does_not_match_original_journal_anchor() {
            use sha2::{Digest, Sha256};
            let f = Fixture::new();
            let path = factory_parent(&f);
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [145; 16]);
            let mut j = factory_journal(&store, &physical);
            let flag = AtomicBool::new(false);
            drop(j.create_workspaces(&physical, &[&path], &flag).unwrap());
            let filename = j.placements().unwrap().path();
            drop(j);
            let original = fs::read(&filename).unwrap();
            let mut forged = original.clone();
            let at = forged
                .windows(b"factory".len())
                .position(|s| s == b"factory")
                .unwrap();
            forged[at] = b'g';
            let end = forged.len() - 32;
            let hash = Sha256::digest(&forged[..end]);
            forged[end..].copy_from_slice(&hash);
            fs::write(&filename, &forged).unwrap();
            let mut registry = store.open_placements(&physical, &flag).unwrap();
            assert!(
                registry
                    .entry(0)
                    .unwrap()
                    .parent_path()
                    .ends_with("gactory")
            );
            registry.renew(&physical, &flag).unwrap();
            let mut j = Journal::open(&store, &physical).unwrap();
            j.renew(&flag).unwrap();
            assert!(j.attach_placements(registry).is_err());
            assert!(j.ensure_current().is_err());
            fs::write(&filename, &original).unwrap();
            let registry = store.open_placements(&physical, &flag).unwrap();
            assert_eq!(registry.entry(0).unwrap().parent_path(), path);
            drop(registry);
            File::options()
                .write(true)
                .open(&filename)
                .unwrap()
                .set_len(MAX_PLACEMENT_BYTES + 1)
                .unwrap();
            assert!(store.open_placements(&physical, &flag).is_err());
        }
        #[test]
        fn placement_registry_legacy_unregistered_tag32_remains_readable_but_new_writer_refuses() {
            use crate::directory_sync_journal::{self as wire, Event};
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [146; 16]);
            let mut j = factory_journal(&store, &physical);
            let flag = AtomicBool::new(false);
            let intent = Event::WorkspaceCreateIntent {
                transaction: 0,
                seal: [3; 32],
            };
            let path = j.path();
            let before = fs::metadata(&path).unwrap().len();
            assert!(j.append(intent, &flag).is_err());
            assert_eq!(fs::metadata(&path).unwrap().len(), before);
            j.ensure_current().unwrap();
            let frame = wire::frame(j.history().sequence + 1, j.history().tail, intent).unwrap();
            drop(j);
            // Legacy v1 declaration bytes: no mkdir or approval is inferred.
            let mut file = File::options().append(true).open(&path).unwrap();
            file.write_all(&frame).unwrap();
            file.sync_all().unwrap();
            drop(file);
            let mut history = Journal::open(&store, &physical).unwrap();
            assert_eq!(history.durability(), State::History);
            assert_eq!(
                history.history().state.workspace_create_guard(),
                Some((0, [3; 32]))
            );
            assert!(history.history().state.placement_seal().is_none());
            history.renew(&flag).unwrap();
            assert!(history.append(intent, &flag).is_err());
            assert!(store.open_placements(&physical, &flag).is_err());
        }
        fn batch_fd_count() -> usize {
            fs::read_dir("/proc/self/fd").unwrap().count()
        }
        #[test]
        fn workspace_batch_shared_parent_retains_one_fd_and_parts_do_not_duplicate_it() {
            // Exact process-wide FD deltas require a process without sibling tests.
            // Keep the normal parallel suite; isolate only this functional FD assertion.
            const CHILD: &str = "FILES_SHARED_PARENT_FD_COUNT_CHILD";
            if std::env::var_os(CHILD).is_none() {
                let status = Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "directory_sync_store::linux::tests::workspace_batch_shared_parent_retains_one_fd_and_parts_do_not_duplicate_it", "--nocapture"])
                    .env(CHILD, "1").status().unwrap();
                assert!(status.success());
                return;
            }
            let f = Fixture::new();
            let path = factory_parent(&f);
            for i in 0..7 {
                fs::write(f.paths[0].join(format!("extra{i}")), b"data").unwrap();
            }
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            let count = physical.transactions().len();
            assert_eq!(count, 8);
            let parents = vec![path.as_path(); count];
            let report = WorkspaceBatchReport::inspect(&physical, &parents).unwrap();
            assert_eq!(report.distinct_paths(), 1);
            assert_eq!(
                report.retained_fd_slots(),
                physical.staging_admission().unwrap().retained_fd_slots() - (count - 1)
            );
            let store = create(&f, &physical, &prepared, [147; 16]);
            let before_owner = batch_fd_count();
            let mut j = factory_journal(&store, &physical);
            let before = batch_fd_count();
            let mut roots = j
                .create_workspaces(&physical, &parents, &AtomicBool::new(false))
                .unwrap();
            assert_eq!(batch_fd_count() - before, count + 1 + 1); // roots + shared parent + registry.
            assert!(
                roots
                    .windows(2)
                    .all(|r| r[0].parent().as_raw_fd() == r[1].parent().as_raw_fd())
            );
            let handle = roots[0].parent_handle();
            let copy = handle.clone();
            let after = batch_fd_count();
            let (inventory, parent, name) = roots.pop().unwrap().into_parts();
            assert_eq!(batch_fd_count(), after);
            assert_eq!(parent.as_raw_fd(), handle.as_raw_fd());
            parent.verify().unwrap();
            assert_eq!(parent.path(), path);
            assert!(!name.is_empty());
            drop(inventory);
            drop(parent);
            drop(roots);
            assert_eq!(batch_fd_count() - before, 2); // one parent kept by clones + registry.
            drop(handle);
            drop(copy);
            assert_eq!(batch_fd_count() - before, 1);
            drop(j);
            assert_eq!(batch_fd_count(), before_owner); // journal file and directory also closed.
        }
        #[test]
        fn workspace_batch_distinct_raw_parent_paths_keep_distinct_bindings() {
            let f = Fixture::new();
            let left = factory_parent(&f);
            let right = f.base.join("FACTORY");
            fs::create_dir(&right).unwrap();
            fs::set_permissions(&right, fs::Permissions::from_mode(0o700)).unwrap();
            fs::write(f.paths[0].join("y"), b"two").unwrap();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            let report = WorkspaceBatchReport::inspect(&physical, &[&left, &right]).unwrap();
            assert_eq!(report.distinct_paths(), 2);
            assert_eq!(
                report.retained_fd_slots(),
                physical.staging_admission().unwrap().retained_fd_slots()
            );
            let store = create(&f, &physical, &prepared, [148; 16]);
            let mut j = factory_journal(&store, &physical);
            let roots = j
                .create_workspaces(&physical, &[&left, &right], &AtomicBool::new(false))
                .unwrap();
            assert_ne!(roots[0].parent().as_raw_fd(), roots[1].parent().as_raw_fd());
            assert_eq!(
                j.placements().unwrap().entry(0).unwrap().parent_path(),
                left
            );
            assert_eq!(
                j.placements().unwrap().entry(1).unwrap().parent_path(),
                right
            );
            fs::rename(&left, f.base.join("retained")).unwrap();
            assert!(roots[0].parent_handle().verify().is_err());
            roots[1].parent_handle().verify().unwrap();
        }
        #[test]
        fn workspace_batch_cached_exclusions_preserve_private_source_and_store_rejection() {
            let f = Fixture::new();
            let path = factory_parent(&f);
            for p in [&f.base, &f.paths[0], &f.paths[1]] {
                fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
            }
            let child = f.paths[0].join("inside");
            fs::create_dir(&child).unwrap();
            fs::set_permissions(&child, fs::Permissions::from_mode(0o700)).unwrap();
            let store_child = f.store.join("inside");
            fs::create_dir(&store_child).unwrap();
            fs::set_permissions(&store_child, fs::Permissions::from_mode(0o700)).unwrap();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [149; 16]);
            let mut j = factory_journal(&store, &physical);
            let length = fs::metadata(j.path()).unwrap().len();
            for invalid in [
                &f.base,
                &f.paths[0],
                &f.paths[1],
                &child,
                &f.store,
                &store_child,
            ] {
                let mut parents = vec![path.as_path(); physical.transactions().len()];
                *parents.last_mut().unwrap() = invalid;
                let err = j
                    .create_workspaces(&physical, &parents, &AtomicBool::new(false))
                    .unwrap_err();
                assert!(!err.intent_acknowledged && !err.syscall_attempted);
                assert_eq!(fs::read_dir(&path).unwrap().count(), 0);
                assert_eq!(fs::metadata(j.path()).unwrap().len(), length);
                j.ensure_current().unwrap();
            }
            assert_eq!(
                fs::read_dir(&f.store)
                    .unwrap()
                    .filter_map(Result::ok)
                    .filter(|e| e.file_name().as_bytes().starts_with(b"sync-placements-"))
                    .count(),
                0
            );
        }
        #[test]
        fn workspace_batch_shared_parent_replacement_after_admission_stops_before_mkdir() {
            use journal::WorkspacePoint;
            let f = Fixture::new();
            let path = factory_parent(&f);
            fs::write(f.paths[0].join("y"), b"two").unwrap();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [150; 16]);
            let mut j = factory_journal(&store, &physical);
            let retained = f.base.join("retained");
            let err = j
                .create_workspaces_with(&physical, &[&path, &path], &AtomicBool::new(false), |at| {
                    if matches!(at, WorkspacePoint::BeforeCreate) {
                        fs::rename(&path, &retained)?;
                        fs::create_dir(&path)?;
                        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
                    }
                    Ok(())
                })
                .unwrap_err();
            assert!(err.intent_acknowledged);
            assert!(!err.syscall_attempted);
            assert_eq!(j.durability(), State::Uncertain);
            assert_eq!(fs::read_dir(&retained).unwrap().count(), 0);
            assert_eq!(fs::read_dir(&path).unwrap().count(), 0);
        }
        #[test]
        fn workspace_batch_shared_fd_admission_succeeds_below_legacy_forecast_in_child() {
            const ENV: &str = "FILES_BATCH_FD_CHILD";
            if std::env::var_os(ENV).is_some() {
                let f = Fixture::new();
                let path = factory_parent(&f);
                for i in 0..7 {
                    fs::write(f.paths[0].join(format!("extra{i}")), b"data").unwrap();
                }
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [151; 16]);
                let mut j = factory_journal(&store, &physical);
                let parents = vec![path.as_path(); physical.transactions().len()];
                let report = WorkspaceBatchReport::inspect(&physical, &parents).unwrap();
                let margin = 4 * crate::directory_sync_admission::MAX_SCOPE_DEPTH + 32;
                let available = report.retained_fd_slots() + 1;
                assert!(
                    physical
                        .staging_admission()
                        .unwrap()
                        .require_fd_slots(available)
                        .is_err()
                );
                let mut old = std::mem::MaybeUninit::<libc::rlimit>::zeroed();
                assert_eq!(
                    unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, old.as_mut_ptr()) },
                    0
                );
                let old = unsafe { old.assume_init() };
                let low = libc::rlimit {
                    rlim_cur: (batch_fd_count() + available + margin) as u64,
                    rlim_max: old.rlim_max,
                };
                assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &low) }, 0);
                let result = j.create_workspaces(&physical, &parents, &AtomicBool::new(false));
                assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &old) }, 0);
                assert_eq!(result.unwrap().len(), 8);
                return;
            }
            let status = Command::new(std::env::current_exe().unwrap()).args(["--exact", "directory_sync_store::linux::tests::workspace_batch_shared_fd_admission_succeeds_below_legacy_forecast_in_child", "--nocapture"]).env(ENV, "1").status().unwrap();
            assert!(status.success());
        }
        #[test]
        fn stage_coordinator_single_file_exact_budget_and_publication_pipeline() {
            use crate::{
                directory_sync_journal::{Event, NamespaceAction},
                exdev_source::{NamespacePath, NamespaceScope, plan_namespace_move},
            };
            let f = Fixture::new();
            let path = factory_parent(&f);
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            let report = StageBatchReport::inspect(&physical, &[&path]).unwrap();
            assert_eq!(report.required_regular_bytes(), 54);
            assert_eq!(report.metadata_steps(), 1);
            assert_eq!(report.copied_regular_bytes(), 6);
            let store = create(&f, &physical, &prepared, [152; 16]);
            let mut j = factory_journal(&store, &physical);
            let flag = AtomicBool::new(false);
            let batch = j
                .stage_batch(&physical, &prepared, &[&path], 54, &flag)
                .unwrap();
            assert_eq!(batch.report(), report);
            assert_eq!(batch.barrier().payload_read(), 12);
            assert!(!f.paths[1].join("x").exists());
            assert_eq!(fs::read(f.paths[0].join("x")).unwrap(), b"source");
            let (mut inventories, locations, mut sources, _) = batch.into_parts();
            for e in [
                Event::EffectIntent(0),
                Event::AbsenceChecked(0),
                Event::PublishIntent(0),
            ] {
                j.append(e, &flag).unwrap();
            }
            let target = &mut sources[1];
            let rename = plan_namespace_move(
                vec![
                    NamespaceScope {
                        inventory: &mut inventories[0],
                        parent: locations[0].parent(),
                        name: locations[0].name(),
                    },
                    NamespaceScope {
                        inventory: &mut target.inventory,
                        parent: &target.parent,
                        name: &target.name,
                    },
                ],
                NamespacePath {
                    scope: 0,
                    relative: b"new",
                },
                NamespacePath {
                    scope: 1,
                    relative: b"x",
                },
                1024,
                &flag,
            )
            .unwrap();
            j.execute_namespace(0, NamespaceAction::Publish, rename, 1024, &flag)
                .unwrap();
            assert_eq!(fs::read(f.paths[1].join("x")).unwrap(), b"source");
        }
        #[test]
        fn batch_namespace_journal_publishes_cross_workspace_file_and_symlink_aliases() {
            use crate::{
                directory_sync_journal::{Event, NamespaceAction},
                exdev_source::{NamespacePath, NamespaceScope, plan_batch_namespace_move},
            };
            use std::ffi::OsStr;
            for after_ack_failure in [false, true] {
                let f = Fixture::new();
                let parent = factory_parent(&f);
                fs::hard_link(f.paths[0].join("x"), f.paths[0].join("y")).unwrap();
                symlink("x", f.paths[0].join("link")).unwrap();
                fs::hard_link(f.paths[0].join("link"), f.paths[0].join("link2")).unwrap();
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let parents = vec![parent.as_path(); physical.transactions().len()];
                let report = StageBatchReport::inspect(&physical, &parents).unwrap();
                let store = create(&f, &physical, &prepared, [163; 16]);
                let mut j = factory_journal(&store, &physical);
                let flag = AtomicBool::new(false);
                let batch = j
                    .stage_batch(
                        &physical,
                        &prepared,
                        &parents,
                        report.required_regular_bytes(),
                        &flag,
                    )
                    .unwrap();
                let (mut inventories, locations, mut sources, _) = batch.into_parts();
                let workspace_count = inventories.len();
                assert_eq!(workspace_count, 4);
                for (i, transaction) in physical.transactions().iter().enumerate() {
                    for e in [
                        Event::EffectIntent(i as u32),
                        Event::AbsenceChecked(i as u32),
                        Event::PublishIntent(i as u32),
                    ] {
                        j.append(e, &flag).unwrap();
                    }
                    let before: Vec<_> = inventories
                        .iter()
                        .map(|v| format!("{:?}", v.entries()))
                        .collect();
                    let mut scopes: Vec<_> = inventories
                        .iter_mut()
                        .zip(&locations)
                        .map(|(inventory, location)| NamespaceScope {
                            inventory,
                            parent: location.parent(),
                            name: location.name(),
                        })
                        .collect();
                    scopes.extend(sources.iter_mut().map(|source| NamespaceScope {
                        inventory: &mut source.inventory,
                        parent: &source.parent,
                        name: &source.name,
                    }));
                    let rename = plan_batch_namespace_move(
                        scopes,
                        NamespacePath {
                            scope: i,
                            relative: b"new",
                        },
                        NamespacePath {
                            scope: workspace_count + 1,
                            relative: physical.plan().name(transaction.row()).unwrap(),
                        },
                        4096,
                        &flag,
                    )
                    .unwrap();
                    let result = j.execute_namespace_with(
                        i as u32,
                        NamespaceAction::Publish,
                        rename,
                        4096,
                        &flag,
                        |point| {
                            if after_ack_failure
                                && point == crate::exdev_source::OwnPoint::Confirmed
                            {
                                Err(std::io::Error::other(
                                    "injected after durable acknowledgement",
                                ))
                            } else {
                                Ok(())
                            }
                        },
                    );
                    if after_ack_failure {
                        let failure = result.unwrap_err();
                        assert!(failure.syscall_succeeded);
                        assert!(
                            fs::symlink_metadata(f.paths[1].join(OsStr::from_bytes(
                                physical.plan().name(transaction.row()).unwrap()
                            )))
                            .is_ok()
                        );
                        for (inventory, before) in inventories.iter().zip(before) {
                            assert_eq!(format!("{:?}", inventory.entries()), before);
                        }
                        break;
                    }
                    result.unwrap();
                    j.append(Event::Committed(i as u32), &flag).unwrap();
                    for (inventory, location) in inventories.iter().zip(&locations) {
                        inventory
                            .verify_for_sync(location.parent(), location.name(), 4096, &flag)
                            .unwrap();
                    }
                    for source in &sources {
                        source
                            .inventory
                            .verify_for_sync(&source.parent, &source.name, 4096, &flag)
                            .unwrap();
                    }
                }
                if !after_ack_failure {
                    use std::os::unix::fs::MetadataExt;
                    for pair in [["x", "y"], ["link", "link2"]] {
                        let a = fs::symlink_metadata(f.paths[1].join(pair[0])).unwrap();
                        let b = fs::symlink_metadata(f.paths[1].join(pair[1])).unwrap();
                        assert_eq!((a.dev(), a.ino(), a.nlink()), (b.dev(), b.ino(), 2));
                    }
                }
            }
        }
        #[test]
        fn stage_coordinator_multiple_workspaces_raw_tree_and_cross_scope_link_cohorts() {
            let f = Fixture::new();
            let path = factory_parent(&f);
            fs::hard_link(f.paths[0].join("x"), f.paths[0].join("y")).unwrap();
            fs::create_dir(f.paths[0].join("d")).unwrap();
            fs::create_dir(f.paths[0].join("d/empty")).unwrap();
            fs::write(f.paths[0].join("d/a"), b"abc").unwrap();
            fs::hard_link(f.paths[0].join("d/a"), f.paths[0].join("d/b")).unwrap();
            let raw = OsString::from_vec(b"raw-\xff\n".to_vec());
            fs::write(f.paths[0].join(&raw), b"raw").unwrap();
            let outside = f.base.join("untouched");
            fs::write(&outside, b"target").unwrap();
            symlink(&outside, f.paths[0].join("link")).unwrap();
            fs::hard_link(f.paths[0].join("link"), f.paths[0].join("link2")).unwrap();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            let parents = vec![path.as_path(); physical.transactions().len()];
            let report = StageBatchReport::inspect(&physical, &parents).unwrap();
            let store = create(&f, &physical, &prepared, [153; 16]);
            let mut j = factory_journal(&store, &physical);
            let batch = j
                .stage_batch(
                    &physical,
                    &prepared,
                    &parents,
                    report.required_regular_bytes(),
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(batch.inventories().len(), physical.transactions().len());
            assert_eq!(batch.report().copied_regular_bytes(), 12);
            assert_eq!(batch.report().staged_regular_bytes(), 21);
            assert_eq!(batch.report().workspace_walk_regular_bytes(), 18);
            let mut file_ids = Vec::new();
            let mut link_ids = Vec::new();
            for inventory in batch.inventories() {
                for e in inventory.entries().iter().skip(1) {
                    if e.identity.kind == Kind::File && e.revision.size == 6 {
                        file_ids.push(e.identity);
                    }
                    if e.identity.kind == Kind::Symlink {
                        link_ids.push(e.identity);
                    }
                }
            }
            assert_eq!(file_ids.len(), 2);
            assert_eq!(file_ids[0], file_ids[1]);
            assert_eq!(link_ids.len(), 2);
            assert_eq!(link_ids[0], link_ids[1]);
            assert_eq!(fs::read(outside).unwrap(), b"target");
            assert_eq!(fs::read_dir(&f.paths[1]).unwrap().count(), 0);
        }
        #[test]
        fn stage_coordinator_rejects_budget_and_private_parent_before_hashing_or_artifacts() {
            for invalid_parent in [false, true] {
                let f = Fixture::new();
                let path = factory_parent(&f);
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let report = StageBatchReport::inspect(&physical, &[&path]).unwrap();
                let store = create(&f, &physical, &prepared, [154; 16]);
                let mut j = factory_journal(&store, &physical);
                let before = fs::metadata(j.path()).unwrap().len();
                if invalid_parent {
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
                }
                let limit = report.required_regular_bytes() - u64::from(!invalid_parent);
                let err = j
                    .stage_batch(
                        &physical,
                        &prepared,
                        &[&path],
                        limit,
                        &AtomicBool::new(false),
                    )
                    .unwrap_err();
                assert_eq!(err.phase, StageBatchPhase::Admission);
                assert_eq!(err.reserved_regular_bytes, 0);
                assert!(!err.private_effects_possible);
                assert_eq!(fs::metadata(j.path()).unwrap().len(), before);
                j.ensure_current().unwrap();
                assert_eq!(fs::read_dir(&path).unwrap().count(), 0);
            }
        }
        #[test]
        fn stage_coordinator_failures_keep_workspaces_and_never_publish_or_retry() {
            for point in 0..11 {
                let f = Fixture::new();
                let path = factory_parent(&f);
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [155; 16]);
                let mut j = factory_journal(&store, &physical);
                let budget = StageBatchReport::inspect(&physical, &[&path])
                    .unwrap()
                    .required_regular_bytes();
                let err = j
                    .stage_batch_inner(
                        &physical,
                        &prepared,
                        &[&path],
                        budget,
                        &AtomicBool::new(false),
                        None,
                        |at| {
                            let fail = match at {
                                StageBatchPoint::Admitted => point == 0,
                                StageBatchPoint::SourcesCaptured => point == 1,
                                StageBatchPoint::WorkspacesCreated => point == 2,
                                StageBatchPoint::BeforeNode(0) => point == 3,
                                StageBatchPoint::Captured => point == 4,
                                StageBatchPoint::MetadataMutation(0) => point == 5,
                                StageBatchPoint::MetadataApplied(0) => point == 6,
                                StageBatchPoint::SourcesRechecked => point == 7,
                                StageBatchPoint::BeforeBarrier => point == 8,
                                StageBatchPoint::Acknowledged => point == 9,
                                StageBatchPoint::BeforeReturn => point == 10,
                                _ => false,
                            };
                            if fail {
                                Err(io::Error::other("coordinator fault"))
                            } else {
                                Ok(())
                            }
                        },
                    )
                    .unwrap_err();
                assert_eq!(err.private_effects_possible, point >= 2);
                assert!(err.reserved_regular_bytes <= budget);
                assert!(!f.paths[1].join("x").exists());
                assert_eq!(fs::read(f.paths[0].join("x")).unwrap(), b"source");
                if point < 2 {
                    j.ensure_current().unwrap();
                    assert_eq!(fs::read_dir(&path).unwrap().count(), 0);
                } else {
                    assert_eq!(j.durability(), State::Uncertain);
                    assert_eq!(fs::read_dir(&path).unwrap().count(), 1);
                    drop(j);
                    let mut history = Journal::open(&store, &physical).unwrap();
                    history.renew(&AtomicBool::new(false)).unwrap();
                    assert!(
                        history
                            .stage_batch(
                                &physical,
                                &prepared,
                                &[&path],
                                budget,
                                &AtomicBool::new(false)
                            )
                            .is_err()
                    );
                }
            }
        }
        #[test]
        fn stage_coordinator_control_pause_resume_and_cancel_precede_private_effects() {
            use crate::copy_engine::Control;
            use std::{sync::mpsc, time::Duration};
            for cancelled in [false, true] {
                let f = Fixture::new();
                let path = factory_parent(&f);
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [156; 16]);
                let mut j = factory_journal(&store, &physical);
                let budget = StageBatchReport::inspect(&physical, &[&path])
                    .unwrap()
                    .required_regular_bytes();
                let control = Control::default();
                control.pause();
                let worker_control = control.clone();
                std::thread::scope(|scope| {
                    let (enter_tx, enter_rx) = mpsc::channel();
                    let (done_tx, done_rx) = mpsc::channel();
                    let owner = &mut j;
                    let p = &physical;
                    let original = &prepared;
                    let parent = &path;
                    let worker = scope.spawn(move || {
                        enter_tx.send(()).unwrap();
                        done_tx
                            .send(owner.stage_batch_controlled(
                                p,
                                original,
                                &[parent],
                                budget,
                                &worker_control,
                            ))
                            .unwrap();
                    });
                    enter_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                    assert!(done_rx.recv_timeout(Duration::from_millis(30)).is_err());
                    assert_eq!(fs::read_dir(&path).unwrap().count(), 0);
                    if cancelled {
                        control.cancel();
                    } else {
                        control.resume();
                    }
                    let result = done_rx.recv_timeout(Duration::from_secs(3)).unwrap();
                    worker.join().unwrap();
                    if cancelled {
                        let err = result.unwrap_err();
                        assert!(!err.private_effects_possible);
                        assert_eq!(err.reserved_regular_bytes, 0);
                    } else {
                        assert_eq!(result.unwrap().inventories().len(), 1);
                    }
                });
                j.ensure_current().unwrap();
            }
        }
        #[test]
        fn stage_coordinator_late_source_change_and_absolute_workspace_rename_fail_closed() {
            for source_change in [false, true] {
                let f = Fixture::new();
                let path = factory_parent(&f);
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [157; 16]);
                let mut j = factory_journal(&store, &physical);
                let budget = StageBatchReport::inspect(&physical, &[&path])
                    .unwrap()
                    .required_regular_bytes();
                let err = j
                    .stage_batch_inner(
                        &physical,
                        &prepared,
                        &[&path],
                        budget,
                        &AtomicBool::new(false),
                        None,
                        |at| {
                            if source_change && matches!(at, StageBatchPoint::MetadataApplied(0)) {
                                fs::write(f.paths[0].join("x"), b"change")?;
                            } else if !source_change && matches!(at, StageBatchPoint::BeforeNode(0))
                            {
                                let root = fs::read_dir(&path)?.next().unwrap()?.path();
                                fs::rename(root, path.join("retained"))?;
                            }
                            Ok(())
                        },
                    )
                    .unwrap_err();
                assert!(err.private_effects_possible);
                assert_eq!(j.durability(), State::Uncertain);
                assert!(!f.paths[1].join("x").exists());
                if source_change {
                    assert_eq!(err.phase, StageBatchPhase::SourceAfter(0));
                    assert_eq!(err.inventories.len(), 1);
                } else {
                    let cause = err.cause.downcast_ref::<StageCreateFailure>().unwrap();
                    assert!(cause.intent_acknowledged);
                    assert!(!cause.syscall_attempted);
                    assert!(!path.join("retained/new").exists());
                }
            }
        }
        #[test]
        fn stage_coordinator_independent_names_and_retire_only_keep_distinct_contracts() {
            use crate::{
                directory_sync::{Mode, Side},
                directory_sync_plan::{Decision, Hardlinks},
            };
            let f = Fixture::new();
            let path = factory_parent(&f);
            fs::hard_link(f.paths[0].join("x"), f.paths[0].join("y")).unwrap();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = Plan::new(
                &record,
                Options {
                    hardlinks: Hardlinks::IndependentNames,
                    ..Options::default()
                },
            )
            .unwrap();
            let physical = Physical::derive(&plan).unwrap();
            let parents = vec![path.as_path(); 2];
            let report = StageBatchReport::inspect(&physical, &parents).unwrap();
            assert_eq!(report.required_regular_bytes(), 120);
            let store = create(&f, &physical, &prepared, [159; 16]);
            let mut j = factory_journal(&store, &physical);
            let batch = j
                .stage_batch(&physical, &prepared, &parents, 120, &AtomicBool::new(false))
                .unwrap();
            assert_ne!(
                batch.inventories()[0].entries()[1].identity,
                batch.inventories()[1].entries()[1].identity
            );
            let f = Fixture::new();
            let path = factory_parent(&f);
            fs::remove_file(f.paths[0].join("x")).unwrap();
            fs::write(f.paths[1].join("x"), b"old").unwrap();
            let flag = AtomicBool::new(false);
            let prepared = capture(
                Intent {
                    version: 1,
                    id: 31,
                    left: f.paths[0].clone(),
                    right: f.paths[1].clone(),
                    expected: None,
                    mode: Mode::Mirror(Side::Left),
                    read_limit: 1024,
                },
                &flag,
                |_| Ok(()),
            )
            .unwrap();
            let record = prepared.record().unwrap();
            let mut plan = Plan::new(&record, Options::default()).unwrap();
            for row in 0..plan.lines().len() {
                let decision = if plan.name(row) == Some(b"x".as_slice()) {
                    Some(Decision::Retire)
                } else if plan.lines()[row].decision() == Decision::Pending {
                    Some(Decision::Take(Side::Left))
                } else {
                    None
                };
                if let Some(decision) = decision {
                    plan.choose(plan.receipt(row).unwrap(), decision).unwrap();
                }
            }
            let physical = Physical::derive(&plan).unwrap();
            let report = StageBatchReport::inspect(&physical, &[&path]).unwrap();
            assert_eq!(report.required_regular_bytes(), 6);
            let store = create(&f, &physical, &prepared, [160; 16]);
            let mut j = factory_journal(&store, &physical);
            let batch = j
                .stage_batch(&physical, &prepared, &[&path], 6, &flag)
                .unwrap();
            assert_eq!(batch.inventories()[0].entries().len(), 1);
            assert_eq!(fs::read(f.paths[1].join("x")).unwrap(), b"old");
        }
        #[test]
        fn stage_coordinator_real_exits_reopen_history_and_preserve_original_destinations() {
            const ENV: &str = "FILES_STAGE_COORDINATOR_EXIT";
            if let Some(base) = std::env::var_os(ENV) {
                let base = PathBuf::from(base);
                let f = Fixture {
                    paths: [base.join("a"), base.join("b")],
                    store: base.join("values"),
                    base,
                };
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let flag = AtomicBool::new(false);
                let mut store = Store::open(&f.store, &physical, [158; 16]).unwrap();
                store.renew(&physical, &flag).unwrap();
                let mut j = Journal::open(&store, &physical).unwrap();
                j.renew(&flag).unwrap();
                let path = f.base.join("factory");
                let budget = StageBatchReport::inspect(&physical, &[&path])
                    .unwrap()
                    .required_regular_bytes();
                let point = std::env::var("FILES_STAGE_COORDINATOR_POINT").unwrap();
                j.stage_batch_inner(&physical, &prepared, &[&path], budget, &flag, None, |at| {
                    if matches!(
                        (point.as_str(), at),
                        ("node", StageBatchPoint::NodeMaterialized(0))
                            | ("metadata", StageBatchPoint::MetadataApplied(0))
                            | ("barrier", StageBatchPoint::Acknowledged)
                    ) {
                        unsafe { libc::_exit(107) }
                    }
                    Ok(())
                })
                .unwrap();
                panic!("exit hook missing");
            }
            for point in ["node", "metadata", "barrier"] {
                let f = Fixture::new();
                let path = factory_parent(&f);
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [158; 16]);
                let j = factory_journal(&store, &physical);
                drop(j);
                drop(store);
                let status=Command::new(std::env::current_exe().unwrap()).args(["--exact","directory_sync_store::linux::tests::stage_coordinator_real_exits_reopen_history_and_preserve_original_destinations","--nocapture"]).env(ENV,&f.base).env("FILES_STAGE_COORDINATOR_POINT",point).status().unwrap();
                assert_eq!(status.code(), Some(107));
                assert!(!f.paths[1].join("x").exists());
                assert_eq!(fs::read_dir(&path).unwrap().count(), 1);
                let store = Store::open(&f.store, &physical, [158; 16]).unwrap();
                let j = Journal::open(&store, &physical).unwrap();
                assert_eq!(j.durability(), State::History);
                assert_eq!(
                    j.history().state.phase() == crate::directory_sync_journal::Phase::Effects,
                    point == "barrier"
                );
                let registry = store
                    .open_placements(&physical, &AtomicBool::new(false))
                    .unwrap();
                assert_eq!(registry.entry(0).unwrap().parent_path(), path);
            }
        }
        fn own_resolve(record: &Record) -> Plan<'_> {
            use crate::directory_sync_plan::Decision;
            let mut plan = Plan::new(record, Options::default()).unwrap();
            for i in 0..plan.lines().len() {
                if plan.lines()[i].decision() == Decision::Pending {
                    let receipt = plan.receipt(i).unwrap();
                    plan.choose(receipt, Decision::Take(crate::directory_sync::Side::Left))
                        .unwrap();
                }
            }
            plan
        }
        fn own_old(f: &Fixture, directory: bool) {
            if directory {
                fs::create_dir(f.paths[1].join("x")).unwrap();
                fs::write(f.paths[1].join("x/a"), b"old").unwrap();
                fs::hard_link(f.paths[1].join("x/a"), f.paths[1].join("x/b")).unwrap();
                symlink("a", f.paths[1].join("x/link")).unwrap();
            } else {
                fs::write(f.paths[1].join("x"), b"old").unwrap();
            }
        }
        fn own_stage_unconfirmed<'s>(
            f: &Fixture,
            physical: &Physical<'_, '_>,
            store: &'s Store,
            prepared: &Prepared,
        ) -> (Journal<'s>, crate::exdev_source::Inventory, File) {
            use crate::directory_sync_journal::{Event, Object, Observation};
            let root = f.base.join("w");
            fs::create_dir(&root).unwrap();
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
            let parent = File::open(&f.base).unwrap();
            let dir = File::open(&root).unwrap();
            let cancel = AtomicBool::new(false);
            let initial = crate::exdev_source::capture_namespace(
                &parent,
                std::ffi::OsStr::new("w"),
                1024 * 1024,
                &cancel,
            )
            .unwrap();
            let mut journal = Journal::create(store, physical, &cancel).unwrap();
            for event in [
                Event::Approval { ticket: [1; 16] },
                Event::WorkspaceIntent(0),
                Event::WorkspaceCreated {
                    transaction: 0,
                    object: Object {
                        identity: initial.entries()[0].identity,
                        mount: initial.mounts().unwrap()[0],
                    },
                },
            ] {
                journal.append(event, &cancel).unwrap();
            }
            let snapshots = [
                prepared
                    .namespace_snapshot(0, 1024 * 1024, &cancel)
                    .unwrap(),
                prepared
                    .namespace_snapshot(1, 1024 * 1024, &cancel)
                    .unwrap(),
            ];
            let mut receipts: Vec<CreatedStageNode> = Vec::new();
            for (i, n) in physical.nodes().iter().enumerate() {
                let leader = n
                    .cohort()
                    .and_then(|g| physical.cohorts()[g].members().first().copied())
                    .filter(|&first| first < i)
                    .map(|first| &receipts[first]);
                let result = journal
                    .create_stage_node(
                        i as u32,
                        &snapshots[n.source().side],
                        &dir,
                        leader,
                        1024 * 1024,
                        &cancel,
                    )
                    .unwrap();
                assert_eq!(result.node(), i as u32);
                receipts.push(result);
            }
            let view = physical.plan().record().view().unwrap();
            let base = physical
                .nodes()
                .first()
                .map(|n| {
                    let l = n.source();
                    view.roots[l.side].entries[l.index].relative
                })
                .unwrap_or(b"");
            let initial = crate::exdev_source::capture_namespace(
                &parent,
                std::ffi::OsStr::new("w"),
                1024 * 1024,
                &cancel,
            )
            .unwrap();
            let mut workspace = initial;
            for i in physical.stage_metadata_order().unwrap() {
                journal
                    .apply_stage_metadata(
                        i as u32,
                        vec![crate::exdev_source::NamespaceScope {
                            inventory: &mut workspace,
                            parent: &parent,
                            name: std::ffi::OsStr::new("w"),
                        }],
                        1024 * 1024,
                        &cancel,
                    )
                    .unwrap();
            }
            for (i, n) in physical.nodes().iter().enumerate() {
                let l = n.source();
                let relative = view.roots[l.side].entries[l.index].relative;
                let mut key = b"w/new".to_vec();
                if relative != base {
                    key.push(b'/');
                    key.extend_from_slice(&relative[base.len() + 1..]);
                }
                let index = workspace
                    .entries()
                    .binary_search_by(|e| e.relative.cmp(&key))
                    .unwrap();
                let e = &workspace.entries()[index];
                journal
                    .append(
                        Event::NodeVerified {
                            node: i as u32,
                            observed: Observation {
                                object: Object {
                                    identity: e.identity,
                                    mount: workspace.mounts().unwrap()[index],
                                },
                                revision: e.revision,
                                content: e.content_digest,
                                metadata: e.metadata_digest,
                            },
                        },
                        &cancel,
                    )
                    .unwrap();
            }
            (journal, workspace, parent)
        }
        fn own_stage<'s>(
            f: &Fixture,
            physical: &Physical<'_, '_>,
            store: &'s Store,
            prepared: &Prepared,
        ) -> (Journal<'s>, crate::exdev_source::Inventory, File) {
            use crate::directory_sync_journal::Event;
            let (mut journal, workspace, parent) =
                own_stage_unconfirmed(f, physical, store, prepared);
            journal
                .confirm_staging(
                    &[StagingWorkspace {
                        inventory: &workspace,
                        parent: &parent,
                        name: std::ffi::OsStr::new("w"),
                    }],
                    1024 * 1024,
                    &AtomicBool::new(false),
                )
                .unwrap();
            journal
                .append(Event::EffectIntent(0), &AtomicBool::new(false))
                .unwrap();
            (journal, workspace, parent)
        }
        fn creation_workspace<'s>(
            f: &Fixture,
            p: &Physical<'_, '_>,
            store: &'s Store,
        ) -> (Journal<'s>, File) {
            use crate::directory_sync_journal::{Event, Object};
            fs::create_dir(f.base.join("w")).unwrap();
            fs::set_permissions(f.base.join("w"), fs::Permissions::from_mode(0o700)).unwrap();
            let dir = File::open(f.base.join("w")).unwrap();
            let parent = File::open(&f.base).unwrap();
            let cancel = AtomicBool::new(false);
            let inventory = crate::exdev_source::capture_namespace(
                &parent,
                std::ffi::OsStr::new("w"),
                1024 * 1024,
                &cancel,
            )
            .unwrap();
            let mut j = Journal::create(store, p, &cancel).unwrap();
            for e in [
                Event::Approval { ticket: [3; 16] },
                Event::WorkspaceIntent(0),
                Event::WorkspaceCreated {
                    transaction: 0,
                    object: Object {
                        identity: inventory.entries()[0].identity,
                        mount: inventory.mounts().unwrap()[0],
                    },
                },
            ] {
                j.append(e, &cancel).unwrap();
            }
            (j, dir)
        }
        fn metadata_workspace<'s>(
            f: &Fixture,
            physical: &Physical<'_, '_>,
            store: &'s Store,
            prepared: &Prepared,
        ) -> (Journal<'s>, crate::exdev_source::Inventory, File) {
            let (mut j, dir) = creation_workspace(f, physical, store);
            let flag = AtomicBool::new(false);
            let snapshots = [
                prepared.namespace_snapshot(0, 1024 * 1024, &flag).unwrap(),
                prepared.namespace_snapshot(1, 1024 * 1024, &flag).unwrap(),
            ];
            let mut receipts: Vec<CreatedStageNode> = Vec::new();
            for (i, n) in physical.nodes().iter().enumerate() {
                let leader = n
                    .cohort()
                    .and_then(|g| physical.cohorts()[g].members().first().copied())
                    .filter(|&first| first < i)
                    .map(|first| &receipts[first]);
                receipts.push(
                    j.create_stage_node(
                        i as u32,
                        &snapshots[n.source().side],
                        &dir,
                        leader,
                        1024 * 1024,
                        &flag,
                    )
                    .unwrap(),
                );
            }
            let parent = File::open(&f.base).unwrap();
            let inv = crate::exdev_source::capture_namespace(
                &parent,
                std::ffi::OsStr::new("w"),
                1024 * 1024,
                &flag,
            )
            .unwrap();
            (j, inv, parent)
        }
        #[test]
        fn journal_metadata_ack_and_full_reconciliation_advance_only_after_durability() {
            use crate::exdev_source::NamespaceScope;
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [121; 16]);
            let (mut j, mut inv, parent) = metadata_workspace(&f, &physical, &store, &prepared);
            let capture = inv.payload_read();
            let flag = AtomicBool::new(false);
            let result = j
                .apply_stage_metadata(
                    0,
                    vec![NamespaceScope {
                        inventory: &mut inv,
                        parent: &parent,
                        name: std::ffi::OsStr::new("w"),
                    }],
                    12,
                    &flag,
                )
                .unwrap();
            assert_eq!(result.node(), 0);
            assert_eq!(result.payload_read(), 12);
            assert_ne!(result.seal(), [0; 32]);
            assert_eq!(result.sequence(), j.history().sequence);
            assert_eq!(result.applied().revision(), inv.entries()[1].revision);
            assert_eq!(result.applied().digest(), inv.entries()[1].metadata_digest);
            assert_eq!(inv.payload_read(), capture);
            assert_eq!(j.history().state.stage_metadata_done(), 1);
            assert!(j.history().state.stage_metadata_guard().is_none());
            inv.verify_for_sync(&parent, std::ffi::OsStr::new("w"), 6, &flag)
                .unwrap();
            assert!(
                j.apply_stage_metadata(
                    0,
                    vec![NamespaceScope {
                        inventory: &mut inv,
                        parent: &parent,
                        name: std::ffi::OsStr::new("w")
                    }],
                    12,
                    &flag
                )
                .is_err()
            );
        }
        #[test]
        fn journal_metadata_faults_preserve_all_original_records_and_poison_owner() {
            use crate::exdev_source::NamespaceScope;
            use journal::MetadataPoint;
            for point in 0..16 {
                let f = Fixture::new();
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [122; 16]);
                let (mut j, mut inv, parent) = metadata_workspace(&f, &physical, &store, &prepared);
                let before = inv.entries()[1].revision;
                let flag = AtomicBool::new(false);
                let err = j
                    .stage_metadata_inner(
                        0,
                        vec![NamespaceScope {
                            inventory: &mut inv,
                            parent: &parent,
                            name: std::ffi::OsStr::new("w"),
                        }],
                        12,
                        &flag,
                        None,
                        |at| {
                            let n = match at {
                                MetadataPoint::IntentWritten => 0,
                                MetadataPoint::IntentFileSynced => 1,
                                MetadataPoint::IntentAcknowledged => 2,
                                MetadataPoint::IntentDurable => 3,
                                MetadataPoint::ValuesPrepared => 4,
                                MetadataPoint::Owner => 5,
                                MetadataPoint::Writable => 6,
                                MetadataPoint::Attributes => 7,
                                MetadataPoint::Mode => 8,
                                MetadataPoint::Times => 9,
                                MetadataPoint::ValuesVerified => 10,
                                MetadataPoint::AppliedWritten => 11,
                                MetadataPoint::AppliedFileSynced => 12,
                                MetadataPoint::AppliedAcknowledged => 13,
                                MetadataPoint::Confirmed => 14,
                                MetadataPoint::BeforeOwnerCommit => 15,
                                MetadataPoint::BeforeMutation => 99,
                            };
                            if n == point {
                                Err(io::Error::other("metadata journal fault"))
                            } else {
                                Ok(())
                            }
                        },
                    )
                    .unwrap_err();
                assert_eq!(err.intent_acknowledged, point >= 3);
                assert_eq!(err.mutation_attempted, point >= 6);
                assert_eq!(err.applied_acknowledged, point >= 14);
                assert_eq!(inv.entries()[1].revision, before);
                assert_eq!(j.durability(), State::Uncertain);
                assert_eq!(fs::read(f.base.join("w/new")).unwrap(), b"source");
                drop(j);
                let reopened = Journal::open(&store, &physical).unwrap();
                assert_eq!(reopened.durability(), State::History);
                assert_eq!(
                    reopened.history().state.stage_metadata_guard().is_some(),
                    point < 11
                );
            }
        }
        #[test]
        fn journal_metadata_rejects_known_budget_missing_scope_and_unrelated_payload_before_intent()
        {
            use crate::exdev_source::NamespaceScope;
            for case in 0..3 {
                let f = Fixture::new();
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [123; 16]);
                let (mut j, mut inv, parent) = metadata_workspace(&f, &physical, &store, &prepared);
                let len = fs::metadata(j.path()).unwrap().len();
                let flag = AtomicBool::new(false);
                if case == 2 {
                    fs::write(f.base.join("w/new"), b"broken").unwrap();
                    inv = crate::exdev_source::capture_namespace(
                        &parent,
                        std::ffi::OsStr::new("w"),
                        1024,
                        &flag,
                    )
                    .unwrap();
                }
                let scopes = if case == 0 {
                    vec![]
                } else {
                    vec![NamespaceScope {
                        inventory: &mut inv,
                        parent: &parent,
                        name: std::ffi::OsStr::new("w"),
                    }]
                };
                let err = j
                    .apply_stage_metadata(0, scopes, if case == 1 { 11 } else { 1024 }, &flag)
                    .unwrap_err();
                assert!(!err.intent_acknowledged);
                assert!(!err.mutation_attempted);
                assert_eq!(fs::metadata(j.path()).unwrap().len(), len);
                assert_eq!(j.durability(), State::Durable);
            }
        }
        #[test]
        fn journal_metadata_codec_and_pending_replay_never_reconstruct_mutation_authority() {
            use crate::directory_sync_journal::{self as wire, Event};
            use crate::exdev_source::NamespaceScope;
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [124; 16]);
            let (mut j, mut inv, parent) = metadata_workspace(&f, &physical, &store, &prepared);
            let flag = AtomicBool::new(false);
            j.stage_metadata_inner(
                0,
                vec![NamespaceScope {
                    inventory: &mut inv,
                    parent: &parent,
                    name: std::ffi::OsStr::new("w"),
                }],
                12,
                &flag,
                None,
                |at| {
                    if matches!(at, journal::MetadataPoint::IntentDurable) {
                        Err(io::Error::other("stop"))
                    } else {
                        Ok(())
                    }
                },
            )
            .unwrap_err();
            drop(j);
            let mut reopened = Journal::open(&store, &physical).unwrap();
            reopened.renew(&flag).unwrap();
            let (_, before, seal) = reopened.history().state.stage_metadata_guard().unwrap();
            for event in [
                Event::StageMetadataIntent {
                    node: 0,
                    before,
                    seal,
                },
                Event::StageMetadataApplied {
                    node: 0,
                    after: before,
                },
            ] {
                let bytes = wire::encode_event(event);
                assert!(bytes.len() <= wire::MAX_EVENT);
                assert_eq!(wire::decode_event(&bytes).unwrap(), event);
                for cut in 0..bytes.len() {
                    assert!(wire::decode_event(&bytes[..cut]).is_err());
                }
                let mut extra = bytes;
                extra.push(0);
                assert!(wire::decode_event(&extra).is_err());
            }
            assert!(
                reopened
                    .apply_stage_metadata(
                        0,
                        vec![NamespaceScope {
                            inventory: &mut inv,
                            parent: &parent,
                            name: std::ffi::OsStr::new("w")
                        }],
                        12,
                        &flag
                    )
                    .is_err()
            );
            assert!(reopened.history().state.stage_metadata_guard().is_some());
        }
        fn metadata_many<'s>(
            f: &Fixture,
            physical: &Physical<'_, '_>,
            store: &'s Store,
            prepared: &Prepared,
        ) -> (
            Journal<'s>,
            Vec<crate::exdev_source::Inventory>,
            Vec<OsString>,
            File,
        ) {
            use crate::directory_sync_journal::{Event, Object};
            let flag = AtomicBool::new(false);
            let parent = File::open(&f.base).unwrap();
            let mut j = Journal::create(store, physical, &flag).unwrap();
            j.append(Event::Approval { ticket: [4; 16] }, &flag)
                .unwrap();
            let mut names = Vec::new();
            let mut dirs = Vec::new();
            for i in 0..physical.transactions().len() {
                let name = OsString::from(format!("w{i}"));
                fs::create_dir(f.base.join(&name)).unwrap();
                fs::set_permissions(f.base.join(&name), fs::Permissions::from_mode(0o700)).unwrap();
                let inv =
                    crate::exdev_source::capture_namespace(&parent, &name, 1024 * 1024, &flag)
                        .unwrap();
                j.append(Event::WorkspaceIntent(i as u32), &flag).unwrap();
                j.append(
                    Event::WorkspaceCreated {
                        transaction: i as u32,
                        object: Object {
                            identity: inv.entries()[0].identity,
                            mount: inv.mounts().unwrap()[0],
                        },
                    },
                    &flag,
                )
                .unwrap();
                dirs.push(File::open(f.base.join(&name)).unwrap());
                names.push(name);
            }
            let snapshots = [
                prepared.namespace_snapshot(0, 1024 * 1024, &flag).unwrap(),
                prepared.namespace_snapshot(1, 1024 * 1024, &flag).unwrap(),
            ];
            let mut receipts: Vec<CreatedStageNode> = Vec::new();
            for (i, n) in physical.nodes().iter().enumerate() {
                let leader = n
                    .cohort()
                    .and_then(|g| physical.cohorts()[g].members().first().copied())
                    .filter(|&v| v < i)
                    .map(|v| &receipts[v]);
                receipts.push(
                    j.create_stage_node(
                        i as u32,
                        &snapshots[n.source().side],
                        &dirs[n.transaction()],
                        leader,
                        1024 * 1024,
                        &flag,
                    )
                    .unwrap(),
                );
            }
            let invs = names
                .iter()
                .map(|name| {
                    crate::exdev_source::capture_namespace(&parent, name, 1024 * 1024, &flag)
                        .unwrap()
                })
                .collect();
            (j, invs, names, parent)
        }
        #[test]
        fn journal_metadata_cross_workspace_aliases_commit_together_or_not_at_all() {
            use crate::exdev_source::NamespaceScope;
            for case in 0..4 {
                let f = Fixture::new();
                fs::write(f.base.join("outside"), b"keep").unwrap();
                if case == 1 {
                    fs::remove_file(f.paths[0].join("x")).unwrap();
                    symlink("../outside", f.paths[0].join("x")).unwrap();
                }
                fs::hard_link(f.paths[0].join("x"), f.paths[0].join("y")).unwrap();
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [125; 16]);
                let (mut j, mut invs, names, parent) =
                    metadata_many(&f, &physical, &store, &prepared);
                let old: Vec<_> = invs.iter().map(|v| v.entries()[1].revision).collect();
                let flag = AtomicBool::new(false);
                let scopes = invs
                    .iter_mut()
                    .zip(&names)
                    .map(|(inventory, name)| NamespaceScope {
                        inventory,
                        parent: &parent,
                        name,
                    })
                    .collect();
                let result = j.stage_metadata_inner(0, scopes, 24, &flag, None, |at| {
                    if case == 2 && matches!(at, journal::MetadataPoint::ValuesVerified) {
                        fs::write(f.base.join("w1/foreign"), b"keep").unwrap();
                    }
                    if case == 3 && matches!(at, journal::MetadataPoint::Confirmed) {
                        fs::rename(f.base.join("w1/new"), f.base.join("w1/kept")).unwrap();
                    }
                    Ok(())
                });
                if case < 2 {
                    let r = result.unwrap();
                    assert_eq!(r.payload_read(), if case == 1 { 0 } else { 24 });
                    assert_eq!(invs[0].entries()[1].revision, invs[1].entries()[1].revision);
                    assert_eq!(invs[0].entries()[1].metadata_digest, r.applied().digest());
                    assert_eq!(invs[1].entries()[1].metadata_digest, r.applied().digest());
                    for (inv, name) in invs.iter().zip(&names) {
                        inv.verify_for_sync(&parent, name, 12, &flag).unwrap();
                    }
                } else {
                    let e = result.unwrap_err();
                    assert!(e.mutation_attempted);
                    assert_eq!(e.applied_acknowledged, case == 3);
                    assert_eq!(j.durability(), State::Uncertain);
                    for (inv, old) in invs.iter().zip(old) {
                        assert_eq!(inv.entries()[1].revision, old);
                    }
                }
                assert_eq!(fs::metadata(f.base.join("outside")).unwrap().nlink(), 1);
                assert_eq!(fs::read(f.base.join("outside")).unwrap(), b"keep");
            }
        }
        #[test]
        fn journal_metadata_order_and_prior_confirmation_cannot_be_rebased_implicitly() {
            use crate::exdev_source::NamespaceScope;
            let f = Fixture::new();
            fs::remove_file(f.paths[0].join("x")).unwrap();
            fs::create_dir(f.paths[0].join("x")).unwrap();
            fs::write(f.paths[0].join("x/a"), b"source").unwrap();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            assert_eq!(physical.stage_metadata_order().unwrap(), vec![1, 0]);
            let store = create(&f, &physical, &prepared, [126; 16]);
            let (mut j, mut inv, parent) = metadata_workspace(&f, &physical, &store, &prepared);
            let flag = AtomicBool::new(false);
            assert!(
                j.apply_stage_metadata(
                    0,
                    vec![NamespaceScope {
                        inventory: &mut inv,
                        parent: &parent,
                        name: std::ffi::OsStr::new("w")
                    }],
                    12,
                    &flag
                )
                .is_err()
            );
            j.apply_stage_metadata(
                1,
                vec![NamespaceScope {
                    inventory: &mut inv,
                    parent: &parent,
                    name: std::ffi::OsStr::new("w"),
                }],
                12,
                &flag,
            )
            .unwrap();
            fs::set_permissions(f.base.join("w/new/a"), fs::Permissions::from_mode(0o600)).unwrap();
            inv = crate::exdev_source::capture_namespace(
                &parent,
                std::ffi::OsStr::new("w"),
                1024,
                &flag,
            )
            .unwrap();
            let len = fs::metadata(j.path()).unwrap().len();
            let err = j
                .apply_stage_metadata(
                    0,
                    vec![NamespaceScope {
                        inventory: &mut inv,
                        parent: &parent,
                        name: std::ffi::OsStr::new("w"),
                    }],
                    12,
                    &flag,
                )
                .unwrap_err();
            assert!(!err.intent_acknowledged);
            assert!(!err.mutation_attempted);
            assert_eq!(fs::metadata(j.path()).unwrap().len(), len);
            assert_eq!(
                fs::metadata(f.base.join("w/new")).unwrap().mode() & 0o7777,
                0o700
            );
        }
        #[test]
        fn journal_metadata_control_and_pre_syscall_guard_leave_unstarted_work_unchanged() {
            use crate::{copy_engine::Control, exdev_source::NamespaceScope};
            use std::{sync::mpsc, time::Duration};
            for cancelled in [false, true] {
                let f = Fixture::new();
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [127; 16]);
                let (mut j, mut inv, parent) = metadata_workspace(&f, &physical, &store, &prepared);
                let control = Control::default();
                control.pause();
                let worker_control = control.clone();
                std::thread::scope(|scope| {
                    let (enter_tx, enter_rx) = mpsc::channel();
                    let (done_tx, done_rx) = mpsc::channel();
                    let owner = &mut j;
                    let inventory = &mut inv;
                    let p = &parent;
                    let worker = scope.spawn(move || {
                        enter_tx.send(()).unwrap();
                        done_tx
                            .send(owner.apply_stage_metadata_controlled(
                                0,
                                vec![NamespaceScope {
                                    inventory,
                                    parent: p,
                                    name: std::ffi::OsStr::new("w"),
                                }],
                                12,
                                &worker_control,
                            ))
                            .unwrap();
                    });
                    enter_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                    assert!(done_rx.recv_timeout(Duration::from_millis(30)).is_err());
                    if cancelled {
                        control.cancel();
                    } else {
                        control.resume();
                    }
                    let result = done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                    if cancelled {
                        let e = result.unwrap_err();
                        assert!(!e.intent_acknowledged);
                        assert!(!e.mutation_attempted);
                    } else {
                        result.unwrap();
                    }
                    worker.join().unwrap();
                });
                assert_eq!(j.durability(), State::Durable);
            }
            for late in [false, true] {
                let f = Fixture::new();
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [128; 16]);
                let (mut j, mut inv, parent) = metadata_workspace(&f, &physical, &store, &prepared);
                let path = j.path();
                let before = inv.entries()[1].revision;
                let flag = AtomicBool::new(false);
                let err = j
                    .stage_metadata_inner(
                        0,
                        vec![NamespaceScope {
                            inventory: &mut inv,
                            parent: &parent,
                            name: std::ffi::OsStr::new("w"),
                        }],
                        12,
                        &flag,
                        None,
                        |at| {
                            if (late && matches!(at, journal::MetadataPoint::BeforeOwnerCommit))
                                || (!late && matches!(at, journal::MetadataPoint::BeforeMutation))
                            {
                                let bytes = fs::read(&path).unwrap();
                                fs::write(&path, bytes).unwrap();
                            }
                            Ok(())
                        },
                    )
                    .unwrap_err();
                assert!(err.intent_acknowledged);
                assert_eq!(err.mutation_attempted, late);
                assert_eq!(err.applied_acknowledged, late);
                assert_eq!(inv.entries()[1].revision, before);
                assert_eq!(
                    fs::metadata(f.base.join("w/new")).unwrap().mode() & 0o7777,
                    if late { 0o644 } else { 0o600 }
                );
                assert_eq!(j.durability(), State::Uncertain);
            }
        }
        #[test]
        fn journal_metadata_real_process_exits_reopen_data_history_without_owner_commit() {
            use crate::exdev_source::NamespaceScope;
            use std::process::Command;
            const ENV: &str = "FILES_METADATA_JOURNAL_CHILD";
            if let Some(base) = std::env::var_os(ENV) {
                let base = PathBuf::from(base);
                let (mut store, saved) =
                    Store::open_saved(&base.join("values"), [129; 16]).unwrap();
                let plan = saved.plan().unwrap();
                let physical = Physical::derive_with_limit(&plan, saved.stage_limit()).unwrap();
                let flag = AtomicBool::new(false);
                store.renew(&physical, &flag).unwrap();
                let mut j = Journal::open(&store, &physical).unwrap();
                j.renew(&flag).unwrap();
                let parent = File::open(&base).unwrap();
                let mut inv = crate::exdev_source::capture_namespace(
                    &parent,
                    std::ffi::OsStr::new("w"),
                    1024,
                    &flag,
                )
                .unwrap();
                let point = std::env::var("FILES_METADATA_JOURNAL_POINT").unwrap();
                j.stage_metadata_inner(
                    0,
                    vec![NamespaceScope {
                        inventory: &mut inv,
                        parent: &parent,
                        name: std::ffi::OsStr::new("w"),
                    }],
                    12,
                    &flag,
                    None,
                    |at| {
                        if matches!(
                            (point.as_str(), at),
                            ("intent", journal::MetadataPoint::IntentDurable)
                                | ("values", journal::MetadataPoint::ValuesVerified)
                                | ("confirmed", journal::MetadataPoint::Confirmed)
                        ) {
                            unsafe { libc::_exit(97) }
                        }
                        Ok(())
                    },
                )
                .unwrap();
                panic!("exit hook missing");
            }
            for point in ["intent", "values", "confirmed"] {
                let f = Fixture::new();
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [129; 16]);
                let (j, _, _) = metadata_workspace(&f, &physical, &store, &prepared);
                drop(j);
                drop(store);
                let status=Command::new(std::env::current_exe().unwrap()).args(["--exact","directory_sync_store::linux::tests::journal_metadata_real_process_exits_reopen_data_history_without_owner_commit","--nocapture"]).env(ENV,&f.base).env("FILES_METADATA_JOURNAL_POINT",point).status().unwrap();
                assert_eq!(status.code(), Some(97));
                assert_eq!(
                    fs::metadata(f.base.join("w/new")).unwrap().mode() & 0o7777,
                    if point == "intent" { 0o600 } else { 0o644 }
                );
                let store = Store::open(&f.store, &physical, [129; 16]).unwrap();
                let j = Journal::open(&store, &physical).unwrap();
                assert_eq!(j.durability(), State::History);
                assert!(j.ensure_current().is_err());
                assert_eq!(
                    j.history().state.stage_metadata_guard().is_some(),
                    point != "confirmed"
                );
            }
        }
        #[test]
        fn stage_creation_materializes_empty_and_multichunk_files_with_durable_guard() {
            for size in [0, 131073] {
                let f = Fixture::new();
                let bytes: Vec<_> = (0..size).map(|n| (n % 251) as u8).collect();
                fs::write(f.paths[0].join("x"), &bytes).unwrap();
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [111; 16]);
                let cancel = AtomicBool::new(false);
                let (mut j, dir) = creation_workspace(&f, &physical, &store);
                let source = prepared
                    .namespace_snapshot(0, 1024 * 1024, &cancel)
                    .unwrap();
                let result = j
                    .create_stage_node(0, &source, &dir, None, size as u64, &cancel)
                    .unwrap();
                assert_eq!(result.evidence(), CreationEvidence::ExclusiveFile);
                assert_eq!(result.copied(), size as u64);
                assert_eq!(result.verified(), size as u64);
                assert_eq!(fs::read(f.base.join("w/new")).unwrap(), bytes);
                assert!(j.history().state.node_create_guard().is_none());
                assert!(
                    j.create_stage_node(0, &source, &dir, None, size as u64, &cancel)
                        .is_err()
                );
                assert!(!f.paths[1].join("x").exists());
            }
        }
        #[test]
        fn stage_creation_fault_boundaries_retain_objects_and_pending_history_without_retry() {
            use journal::CreatePoint;
            for point in 0..10 {
                let f = Fixture::new();
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [112; 16]);
                let cancel = AtomicBool::new(false);
                let (mut j, dir) = creation_workspace(&f, &physical, &store);
                let source = prepared.namespace_snapshot(0, 1024, &cancel).unwrap();
                let err = j
                    .create_stage_inner(0, &source, &dir, None, 1024, &cancel, None, |at| {
                        let n = match at {
                            CreatePoint::IntentWritten => 0,
                            CreatePoint::IntentFileSynced => 1,
                            CreatePoint::IntentAcknowledged => 2,
                            CreatePoint::IntentDurable => 3,
                            CreatePoint::BeforeCreate => 4,
                            CreatePoint::Created => 5,
                            CreatePoint::Recorded => 6,
                            CreatePoint::Payload => 7,
                            CreatePoint::DataSynced => 8,
                            CreatePoint::Ready => 9,
                        };
                        if n == point {
                            Err(io::Error::other("creation fault"))
                        } else {
                            Ok(())
                        }
                    })
                    .unwrap_err();
                assert_eq!(err.intent_acknowledged, point >= 3);
                assert_eq!(err.syscall_attempted, point >= 5);
                assert_eq!(err.syscall_succeeded, point >= 5);
                assert_eq!(err.identity_recorded, point >= 6);
                assert_eq!(j.durability(), State::Uncertain);
                assert_eq!(f.base.join("w/new").exists(), point >= 5);
                assert_eq!(fs::read(f.paths[0].join("x")).unwrap(), b"source");
                drop(j);
                let mut reopened = Journal::open(&store, &physical).unwrap();
                assert_eq!(reopened.durability(), State::History);
                reopened.renew(&cancel).unwrap();
                assert!(
                    reopened
                        .create_stage_node(0, &source, &dir, None, 1024, &cancel)
                        .is_err()
                );
                assert_eq!(
                    reopened.history().state.node_create_guard().is_some(),
                    point < 9
                );
            }
        }
        #[test]
        fn stage_creation_collision_source_tamper_and_known_budget_fail_without_overwrite() {
            for case in 0..5 {
                let f = Fixture::new();
                if case == 4 {
                    fs::remove_file(f.paths[0].join("x")).unwrap();
                    symlink("missing", f.paths[0].join("x")).unwrap();
                }
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [113; 16]);
                let cancel = AtomicBool::new(false);
                let (mut j, dir) = creation_workspace(&f, &physical, &store);
                let source = prepared.namespace_snapshot(0, 1024, &cancel).unwrap();
                let len = fs::metadata(j.path()).unwrap().len();
                if case == 0 {
                    fs::write(f.base.join("w/new"), b"foreign").unwrap();
                }
                if case == 1 {
                    fs::write(f.paths[0].join("x"), b"changed").unwrap();
                }
                let err = j
                    .create_stage_inner(
                        0,
                        &source,
                        &dir,
                        None,
                        if case == 2 { 5 } else { 1024 },
                        &cancel,
                        None,
                        |at| {
                            if case >= 3 && matches!(at, journal::CreatePoint::BeforeCreate) {
                                fs::write(f.base.join("w/new"), b"foreign").unwrap();
                            }
                            Ok(())
                        },
                    )
                    .unwrap_err();
                assert!(!err.syscall_succeeded);
                assert_eq!(err.syscall_attempted, case >= 3);
                assert_eq!(
                    j.durability(),
                    if case >= 3 {
                        State::Uncertain
                    } else {
                        State::Durable
                    }
                );
                if case < 3 {
                    assert_eq!(fs::metadata(j.path()).unwrap().len(), len);
                }
                if case == 0 || case >= 3 {
                    assert_eq!(fs::read(f.base.join("w/new")).unwrap(), b"foreign");
                }
            }
        }
        #[test]
        fn stage_creation_codec_rejects_cuts_and_materialization_before_identity() {
            use crate::directory_sync_journal::{self as wire, Event};
            for event in [
                Event::NodeCreateIntent {
                    node: 7,
                    seal: [6; 32],
                },
                Event::NodeMaterialized(7),
            ] {
                let bytes = wire::encode_event(event);
                assert_eq!(wire::decode_event(&bytes).unwrap(), event);
                for cut in 0..bytes.len() {
                    assert!(wire::decode_event(&bytes[..cut]).is_err());
                }
                let mut extra = bytes;
                extra.push(0);
                assert!(wire::decode_event(&extra).is_err());
            }
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [114; 16]);
            let cancel = AtomicBool::new(false);
            let (mut j, _) = creation_workspace(&f, &physical, &store);
            assert!(j.append(Event::NodeMaterialized(0), &cancel).is_err());
            j.append(
                Event::NodeCreateIntent {
                    node: 0,
                    seal: [7; 32],
                },
                &cancel,
            )
            .unwrap();
            assert!(j.append(Event::NodeMaterialized(0), &cancel).is_err());
            assert!(
                j.append(
                    Event::NodeCreateIntent {
                        node: 0,
                        seal: [7; 32]
                    },
                    &cancel
                )
                .is_err()
            );
        }
        #[test]
        fn stage_creation_cross_workspace_file_and_symlink_cohorts_link_held_descriptors() {
            use crate::directory_sync_journal::{Event, Object};
            for link in [false, true] {
                let f = Fixture::new();
                fs::write(f.base.join("outside"), b"untouched").unwrap();
                if link {
                    fs::remove_file(f.paths[0].join("x")).unwrap();
                    symlink("../outside", f.paths[0].join("x")).unwrap();
                }
                fs::hard_link(f.paths[0].join("x"), f.paths[0].join("y")).unwrap();
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                assert_eq!(physical.transactions().len(), 2);
                assert_eq!(physical.nodes()[0].cohort(), physical.nodes()[1].cohort());
                assert!(physical.nodes()[0].cohort().is_some());
                let store = create(&f, &physical, &prepared, [115; 16]);
                let flag = AtomicBool::new(false);
                let parent = File::open(&f.base).unwrap();
                let mut j = Journal::create(&store, &physical, &flag).unwrap();
                j.append(Event::Approval { ticket: [3; 16] }, &flag)
                    .unwrap();
                let mut dirs = Vec::new();
                for i in 0..2 {
                    let name = format!("w{i}");
                    fs::create_dir(f.base.join(&name)).unwrap();
                    fs::set_permissions(f.base.join(&name), fs::Permissions::from_mode(0o700))
                        .unwrap();
                    let d = File::open(f.base.join(&name)).unwrap();
                    let inv = crate::exdev_source::capture_namespace(
                        &parent,
                        std::ffi::OsStr::new(&name),
                        1024,
                        &flag,
                    )
                    .unwrap();
                    j.append(Event::WorkspaceIntent(i), &flag).unwrap();
                    j.append(
                        Event::WorkspaceCreated {
                            transaction: i,
                            object: Object {
                                identity: inv.entries()[0].identity,
                                mount: inv.mounts().unwrap()[0],
                            },
                        },
                        &flag,
                    )
                    .unwrap();
                    dirs.push(d);
                }
                let source = prepared.namespace_snapshot(0, 1024, &flag).unwrap();
                let first = j
                    .create_stage_node(0, &source, &dirs[0], None, 1024, &flag)
                    .unwrap();
                let second = j
                    .create_stage_node(1, &source, &dirs[1], Some(&first), 1024, &flag)
                    .unwrap();
                assert_eq!(second.evidence(), CreationEvidence::LinkedHeldDescriptor);
                assert_eq!(first.object(), second.object());
                assert_eq!(second.copied(), 0);
                let a = fs::symlink_metadata(f.base.join("w0/new")).unwrap();
                let b = fs::symlink_metadata(f.base.join("w1/new")).unwrap();
                assert_eq!(a.ino(), b.ino());
                assert_eq!(a.nlink(), 2);
                if link {
                    assert_eq!(
                        fs::read_link(f.base.join("w1/new")).unwrap(),
                        Path::new("../outside")
                    );
                    assert_eq!(fs::metadata(f.base.join("outside")).unwrap().nlink(), 1);
                } else {
                    assert_eq!(second.verified(), 6);
                }
                assert_eq!(fs::read(f.base.join("outside")).unwrap(), b"untouched");
            }
        }
        #[test]
        fn stage_creation_named_postimage_and_late_file_replacement_have_explicit_limits() {
            for directory in [false, true] {
                let f = Fixture::new();
                if directory {
                    fs::remove_file(f.paths[0].join("x")).unwrap();
                    fs::create_dir(f.paths[0].join("x")).unwrap();
                    fs::write(f.paths[0].join("x/a"), b"source").unwrap();
                }
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [116; 16]);
                let flag = AtomicBool::new(false);
                let (mut j, dir) = creation_workspace(&f, &physical, &store);
                let source = prepared.namespace_snapshot(0, 1024, &flag).unwrap();
                let err = j
                    .create_stage_inner(0, &source, &dir, None, 1024, &flag, None, |at| {
                        if matches!(at, journal::CreatePoint::Created) {
                            fs::rename(f.base.join("w/new"), f.base.join("w/kept")).unwrap();
                            if directory {
                                fs::create_dir(f.base.join("w/new")).unwrap();
                                fs::set_permissions(
                                    f.base.join("w/new"),
                                    fs::Permissions::from_mode(0o700),
                                )
                                .unwrap();
                                fs::write(f.base.join("w/new/foreign"), b"keep").unwrap();
                            } else {
                                fs::write(f.base.join("w/new"), b"foreign").unwrap();
                            }
                        }
                        Ok(())
                    })
                    .unwrap_err();
                assert!(err.syscall_succeeded);
                assert!(!err.identity_recorded);
                if directory {
                    assert_eq!(fs::read(f.base.join("w/new/foreign")).unwrap(), b"keep");
                } else {
                    assert_eq!(fs::read(f.base.join("w/new")).unwrap(), b"foreign");
                    assert_eq!(fs::metadata(f.base.join("w/kept")).unwrap().len(), 0);
                }
            }
            // mkdirat has no returned FD: explicitly distinguish observed named
            // postimages from the stronger O_EXCL file receipt.
            let f = Fixture::new();
            fs::remove_file(f.paths[0].join("x")).unwrap();
            fs::create_dir(f.paths[0].join("x")).unwrap();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [117; 16]);
            let flag = AtomicBool::new(false);
            let (mut j, dir) = creation_workspace(&f, &physical, &store);
            let source = prepared.namespace_snapshot(0, 1024, &flag).unwrap();
            let r = j
                .create_stage_node(0, &source, &dir, None, 1024, &flag)
                .unwrap();
            assert_eq!(r.evidence(), CreationEvidence::NamedPostimage);
        }
        #[test]
        fn stage_creation_control_pause_resume_cancel_and_source_change_after_intent() {
            use crate::copy_engine::Control;
            use std::{sync::mpsc, time::Duration};
            for cancelled in [false, true] {
                let f = Fixture::new();
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [118; 16]);
                let flag = AtomicBool::new(false);
                let (mut j, dir) = creation_workspace(&f, &physical, &store);
                let source = prepared.namespace_snapshot(0, 1024, &flag).unwrap();
                let control = Control::default();
                control.pause();
                let worker_control = control.clone();
                std::thread::scope(|scope| {
                    let (enter_tx, enter_rx) = mpsc::channel();
                    let (done_tx, done_rx) = mpsc::channel();
                    let owner = &mut j;
                    let input = &source;
                    let workspace = &dir;
                    let worker = scope.spawn(move || {
                        enter_tx.send(()).unwrap();
                        done_tx
                            .send(owner.create_stage_node_controlled(
                                0,
                                input,
                                workspace,
                                None,
                                1024,
                                &worker_control,
                            ))
                            .unwrap();
                    });
                    enter_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                    assert!(done_rx.recv_timeout(Duration::from_millis(30)).is_err());
                    assert!(!f.base.join("w/new").exists());
                    if cancelled {
                        control.cancel();
                    } else {
                        control.resume();
                    }
                    let result = done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                    if cancelled {
                        let e = result.unwrap_err();
                        assert!(!e.intent_acknowledged);
                        assert!(!e.syscall_attempted);
                    } else {
                        result.unwrap();
                    }
                    worker.join().unwrap();
                });
                assert_eq!(j.durability(), State::Durable);
            }
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [119; 16]);
            let flag = AtomicBool::new(false);
            let (mut j, dir) = creation_workspace(&f, &physical, &store);
            let source = prepared.namespace_snapshot(0, 1024, &flag).unwrap();
            let e = j
                .create_stage_inner(0, &source, &dir, None, 1024, &flag, None, |at| {
                    if matches!(at, journal::CreatePoint::IntentDurable) {
                        fs::write(f.paths[0].join("x"), b"changed").unwrap();
                    }
                    Ok(())
                })
                .unwrap_err();
            assert!(e.intent_acknowledged);
            assert!(!e.syscall_attempted);
            assert!(!f.base.join("w/new").exists());
        }
        #[test]
        fn stage_creation_process_exit_before_identity_after_identity_and_ready_reopens_history() {
            use std::process::Command;
            const ENV: &str = "FILES_STAGE_CREATION_CHILD";
            if let Some(base) = std::env::var_os(ENV) {
                let base = PathBuf::from(base);
                let (mut store, saved) =
                    Store::open_saved(&base.join("values"), [120; 16]).unwrap();
                let plan = saved.plan().unwrap();
                let physical = Physical::derive_with_limit(&plan, saved.stage_limit()).unwrap();
                let flag = AtomicBool::new(false);
                store.renew(&physical, &flag).unwrap();
                let mut j = Journal::open(&store, &physical).unwrap();
                j.renew(&flag).unwrap();
                let parent = File::open(&base).unwrap();
                let inventory = crate::exdev_source::capture_namespace(
                    &parent,
                    std::ffi::OsStr::new("a"),
                    1024,
                    &flag,
                )
                .unwrap();
                let payload_read = inventory.payload_read();
                let source = crate::directory_sync_manifest::NamespaceSnapshot {
                    inventory,
                    parent,
                    name: OsString::from("a"),
                    payload_read,
                };
                let dir = File::open(base.join("w")).unwrap();
                let point = std::env::var("FILES_STAGE_CREATION_POINT").unwrap();
                j.create_stage_inner(0, &source, &dir, None, 1024, &flag, None, |at| {
                    if matches!(
                        (point.as_str(), at),
                        ("created", journal::CreatePoint::Created)
                            | ("recorded", journal::CreatePoint::Recorded)
                            | ("ready", journal::CreatePoint::Ready)
                    ) {
                        unsafe { libc::_exit(89) }
                    }
                    Ok(())
                })
                .unwrap();
                panic!("exit hook missing");
            }
            for point in ["created", "recorded", "ready"] {
                let f = Fixture::new();
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [120; 16]);
                let (j, _) = creation_workspace(&f, &physical, &store);
                drop(j);
                drop(store);
                let status=Command::new(std::env::current_exe().unwrap()).args(["--exact","directory_sync_store::linux::tests::stage_creation_process_exit_before_identity_after_identity_and_ready_reopens_history","--nocapture"]).env(ENV,&f.base).env("FILES_STAGE_CREATION_POINT",point).status().unwrap();
                assert_eq!(status.code(), Some(89));
                assert!(f.base.join("w/new").exists());
                assert_eq!(
                    fs::metadata(f.base.join("w/new")).unwrap().len(),
                    if point == "ready" { 6 } else { 0 }
                );
                let store = Store::open(&f.store, &physical, [120; 16]).unwrap();
                let j = Journal::open(&store, &physical).unwrap();
                assert_eq!(j.durability(), State::History);
                assert!(j.ensure_current().is_err());
                assert_eq!(
                    j.history().state.node_create_guard().is_some(),
                    point != "ready"
                );
            }
        }
        #[test]
        fn staging_barrier_rechecks_disk_and_rejects_extra_or_changed_objects() {
            for case in 0..4 {
                let f = Fixture::new();
                let prepared = f.prepare();
                let record = Record::from_prepared(&prepared).unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [101; 16]);
                let (mut j, w, p) = own_stage_unconfirmed(&f, &physical, &store, &prepared);
                let length = fs::metadata(j.path()).unwrap().len();
                match case {
                    0 => fs::write(f.base.join("w/new"), b"tamper").unwrap(),
                    1 => fs::write(f.base.join("w/extra"), b"extra").unwrap(),
                    2 => fs::set_permissions(f.base.join("w"), fs::Permissions::from_mode(0o755))
                        .unwrap(),
                    _ => fs::rename(f.base.join("w"), f.base.join("relocated")).unwrap(),
                }
                assert!(
                    j.confirm_staging(
                        &[StagingWorkspace {
                            inventory: &w,
                            parent: &p,
                            name: std::ffi::OsStr::new("w")
                        }],
                        1024,
                        &AtomicBool::new(false)
                    )
                    .is_err()
                );
                assert_eq!(fs::metadata(j.path()).unwrap().len(), length);
                assert_eq!(
                    j.history().state.phase(),
                    crate::directory_sync_journal::Phase::Staging
                );
                assert_eq!(j.durability(), State::Durable);
                assert!(!f.paths[1].join("x").exists());
            }
        }
        #[test]
        fn staging_barrier_checks_each_boundary_and_never_returns_receipt_on_fault() {
            use journal::StagingPoint;
            for point in 0..4 {
                let f = Fixture::new();
                let prepared = f.prepare();
                let record = Record::from_prepared(&prepared).unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [102; 16]);
                let (mut j, w, p) = own_stage_unconfirmed(&f, &physical, &store, &prepared);
                let length = fs::metadata(j.path()).unwrap().len();
                assert!(
                    j.confirm_staging_with(
                        &[StagingWorkspace {
                            inventory: &w,
                            parent: &p,
                            name: std::ffi::OsStr::new("w")
                        }],
                        1024,
                        &AtomicBool::new(false),
                        |at| {
                            let n = match at {
                                StagingPoint::Verified => 0,
                                StagingPoint::Synced => 1,
                                StagingPoint::Reverified => 2,
                                StagingPoint::Acknowledged => 3,
                                StagingPoint::WorkspaceAcknowledged => 4,
                            };
                            if n == point {
                                Err(io::Error::other("staging fault"))
                            } else {
                                Ok(())
                            }
                        }
                    )
                    .is_err()
                );
                assert_eq!(
                    j.durability(),
                    if point == 3 {
                        State::Uncertain
                    } else {
                        State::Durable
                    }
                );
                if point < 3 {
                    assert_eq!(fs::metadata(j.path()).unwrap().len(), length);
                }
                assert_eq!(fs::read(f.base.join("w/new")).unwrap(), b"source");
                assert!(!f.paths[1].join("x").exists());
                drop(j);
                let reopened = Journal::open(&store, &physical).unwrap();
                assert_eq!(reopened.durability(), State::History);
            }
        }
        #[test]
        fn staging_barrier_detects_mutation_after_sync_and_enforces_shared_read_allowance() {
            for mode in 0..3 {
                let f = Fixture::new();
                let prepared = f.prepare();
                let record = Record::from_prepared(&prepared).unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [103; 16]);
                let (mut j, w, p) = own_stage_unconfirmed(&f, &physical, &store, &prepared);
                let length = fs::metadata(j.path()).unwrap().len();
                let cancel = AtomicBool::new(false);
                let result = j.confirm_staging_with(
                    &[StagingWorkspace {
                        inventory: &w,
                        parent: &p,
                        name: std::ffi::OsStr::new("w"),
                    }],
                    if mode == 1 { 11 } else { 1024 },
                    &cancel,
                    |at| {
                        if matches!(at, journal::StagingPoint::Synced) {
                            if mode == 0 {
                                fs::write(f.base.join("w/new"), b"source").unwrap();
                            }
                            if mode == 2 {
                                cancel.store(true, Ordering::Relaxed);
                            }
                        }
                        Ok(())
                    },
                );
                assert!(result.is_err());
                assert_eq!(j.durability(), State::Durable);
                assert_eq!(fs::metadata(j.path()).unwrap().len(), length);
            }
        }
        #[test]
        fn staging_barrier_covers_all_workspaces_as_one_shared_budget() {
            use crate::directory_sync_journal::{Event, Object, Observation, Phase};
            use std::ffi::OsStr;
            for case in 0..5 {
                let f = Fixture::new();
                fs::write(f.paths[0].join("y"), b"source").unwrap();
                let prepared = f.prepare();
                let record = Record::from_prepared(&prepared).unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                assert_eq!(physical.transactions().len(), 2);
                assert_eq!(physical.nodes().len(), 2);
                let store = create(&f, &physical, &prepared, [105; 16]);
                let cancel = AtomicBool::new(false);
                let parent = File::open(&f.base).unwrap();
                let expected = StoredProof::decode(&store.proof().encode()).unwrap();
                let mut captures = Vec::new();
                let mut names = Vec::new();
                for i in 0..2 {
                    let name = format!("w{i}");
                    let root = f.base.join(&name);
                    fs::create_dir(&root).unwrap();
                    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
                    fs::write(root.join("new"), b"source").unwrap();
                    let value = store.value(&expected, Target::Stage(i)).unwrap();
                    let v = value.values();
                    assert!(v.attributes().is_empty());
                    fs::set_permissions(root.join("new"), fs::Permissions::from_mode(v.mode()))
                        .unwrap();
                    let times = [
                        libc::timespec {
                            tv_sec: v.access().seconds,
                            tv_nsec: i64::from(v.access().nanos),
                        },
                        libc::timespec {
                            tv_sec: v.modified().seconds,
                            tv_nsec: i64::from(v.modified().nanos),
                        },
                    ];
                    assert_eq!(
                        unsafe {
                            libc::utimensat(
                                libc::AT_FDCWD,
                                c(root.join("new").as_os_str().as_bytes()).unwrap().as_ptr(),
                                times.as_ptr(),
                                libc::AT_SYMLINK_NOFOLLOW,
                            )
                        },
                        0
                    );
                    captures.push(
                        crate::exdev_source::capture_namespace(
                            &parent,
                            OsStr::new(&name),
                            1024,
                            &cancel,
                        )
                        .unwrap(),
                    );
                    names.push(name);
                }
                let observation = |t: usize, i: usize| {
                    let e = &captures[t].entries()[i];
                    Observation {
                        object: Object {
                            identity: e.identity,
                            mount: captures[t].mounts().unwrap()[i],
                        },
                        revision: e.revision,
                        content: e.content_digest,
                        metadata: e.metadata_digest,
                    }
                };
                let mut j = Journal::create(&store, &physical, &cancel).unwrap();
                j.append(Event::Approval { ticket: [2; 16] }, &cancel)
                    .unwrap();
                for i in 0..2 {
                    j.append(Event::WorkspaceIntent(i), &cancel).unwrap();
                    j.append(
                        Event::WorkspaceCreated {
                            transaction: i,
                            object: observation(i as usize, 0).object,
                        },
                        &cancel,
                    )
                    .unwrap();
                }
                for i in 0..2 {
                    j.append(
                        Event::NodeCreated {
                            node: i,
                            object: observation(i as usize, 1).object,
                        },
                        &cancel,
                    )
                    .unwrap();
                }
                for i in 0..2 {
                    j.append(
                        Event::NodeVerified {
                            node: i,
                            observed: observation(i as usize, 1),
                        },
                        &cancel,
                    )
                    .unwrap();
                }
                let scopes: Vec<_> = captures
                    .iter()
                    .zip(&names)
                    .map(|(inventory, name)| StagingWorkspace {
                        inventory,
                        parent: &parent,
                        name: OsStr::new(name),
                    })
                    .collect();
                let len = fs::metadata(j.path()).unwrap().len();
                let result = if case == 1 {
                    j.confirm_staging(&scopes[..1], 24, &cancel)
                } else {
                    j.confirm_staging_with(
                        &scopes,
                        if case == 2 { 23 } else { 24 },
                        &cancel,
                        |at| {
                            if case == 4
                                && matches!(at, journal::StagingPoint::WorkspaceAcknowledged)
                            {
                                return Err(io::Error::other("partial staging ACK"));
                            }
                            if case == 3 && matches!(at, journal::StagingPoint::Synced) {
                                fs::write(f.base.join("w1/new"), b"broken").unwrap();
                            }
                            Ok(())
                        },
                    )
                };
                if case == 0 {
                    let r = result.unwrap();
                    assert_eq!(r.payload_read(), 24);
                    assert_eq!(j.history().state.phase(), Phase::Effects);
                } else {
                    assert!(result.is_err());
                    if case == 4 {
                        assert!(fs::metadata(j.path()).unwrap().len() > len);
                        assert_eq!(j.durability(), State::Uncertain);
                        drop(j);
                        let mut reopened = Journal::open(&store, &physical).unwrap();
                        assert_eq!(reopened.history().state.phase(), Phase::Staging);
                        reopened.renew(&cancel).unwrap();
                        assert!(reopened.confirm_staging(&scopes, 24, &cancel).is_err());
                    } else {
                        assert_eq!(fs::metadata(j.path()).unwrap().len(), len);
                        assert_eq!(j.history().state.phase(), Phase::Staging);
                    }
                }
                assert!(!f.paths[1].join("x").exists());
                assert!(!f.paths[1].join("y").exists());
            }
        }
        #[test]
        fn staging_barrier_rejects_fresh_but_unapproved_values_and_duplicate_scopes() {
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = Record::from_prepared(&prepared).unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [106; 16]);
            let (mut j, old, p) = own_stage_unconfirmed(&f, &physical, &store, &prepared);
            let cancel = AtomicBool::new(false);
            fs::set_permissions(f.base.join("w/new"), fs::Permissions::from_mode(0o600)).unwrap();
            let fresh = crate::exdev_source::capture_namespace(
                &p,
                std::ffi::OsStr::new("w"),
                1024,
                &cancel,
            )
            .unwrap();
            fresh
                .verify_for_sync(&p, std::ffi::OsStr::new("w"), 1024, &cancel)
                .unwrap();
            let before = fs::metadata(j.path()).unwrap().len();
            let scope = StagingWorkspace {
                inventory: &fresh,
                parent: &p,
                name: std::ffi::OsStr::new("w"),
            };
            assert!(j.confirm_staging(&[scope], 1024, &cancel).is_err());
            assert!(
                j.confirm_staging(
                    &[
                        StagingWorkspace {
                            inventory: &old,
                            parent: &p,
                            name: std::ffi::OsStr::new("w")
                        },
                        StagingWorkspace {
                            inventory: &old,
                            parent: &p,
                            name: std::ffi::OsStr::new("w")
                        }
                    ],
                    1024,
                    &cancel
                )
                .is_err()
            );
            assert_eq!(fs::metadata(j.path()).unwrap().len(), before);
            assert_eq!(j.durability(), State::Durable);
        }
        #[test]
        fn staging_barrier_control_pauses_resumes_and_cancels_before_declarations() {
            use crate::copy_engine::Control;
            use std::{sync::mpsc, time::Duration};
            for cancelled in [false, true] {
                let f = Fixture::new();
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [107; 16]);
                let (mut j, w, p) = own_stage_unconfirmed(&f, &physical, &store, &prepared);
                let control = Control::default();
                control.pause();
                let worker_control = control.clone();
                let before = fs::metadata(j.path()).unwrap().len();
                std::thread::scope(|scope| {
                    let (entered_tx, entered_rx) = mpsc::channel();
                    let (done_tx, done_rx) = mpsc::channel();
                    let owner = &mut j;
                    let inventory = &w;
                    let parent = &p;
                    let worker = scope.spawn(move || {
                        entered_tx.send(()).unwrap();
                        let result = owner.confirm_staging_controlled(
                            &[StagingWorkspace {
                                inventory,
                                parent,
                                name: std::ffi::OsStr::new("w"),
                            }],
                            12,
                            &worker_control,
                        );
                        done_tx.send(result).unwrap();
                    });
                    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                    assert!(done_rx.recv_timeout(Duration::from_millis(30)).is_err());
                    if cancelled {
                        control.cancel();
                    } else {
                        control.resume();
                    }
                    let result = done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                    if cancelled {
                        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
                    } else {
                        assert_eq!(result.unwrap().payload_read(), 12);
                    }
                    worker.join().unwrap();
                });
                assert_eq!(j.durability(), State::Durable);
                if cancelled {
                    assert_eq!(fs::metadata(j.path()).unwrap().len(), before);
                }
            }
        }
        #[test]
        fn staging_barrier_receipt_is_historical_and_cannot_repeat_or_survive_history() {
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = Record::from_prepared(&prepared).unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [104; 16]);
            let (mut j, w, p) = own_stage_unconfirmed(&f, &physical, &store, &prepared);
            let scopes = [StagingWorkspace {
                inventory: &w,
                parent: &p,
                name: std::ffi::OsStr::new("w"),
            }];
            let cancel = AtomicBool::new(false);
            assert!(j.confirm_staging(&[], 1024, &cancel).is_err());
            let receipt = j.confirm_staging(&scopes, 12, &cancel).unwrap();
            assert_eq!(receipt.payload_read(), 12);
            assert_ne!(receipt.seal(), [0; 32]);
            assert_eq!(receipt.sequence(), j.history().sequence);
            assert!(j.confirm_staging(&scopes, 12, &cancel).is_err());
            drop(j);
            let mut reopened = Journal::open(&store, &physical).unwrap();
            assert!(reopened.confirm_staging(&scopes, 12, &cancel).is_err());
            reopened.renew(&cancel).unwrap();
            assert!(reopened.confirm_staging(&scopes, 12, &cancel).is_err());
        }
        #[test]
        fn owned_namespace_preserve_publish_file_and_directory_are_durable_before_receipt() {
            use crate::{
                directory_sync_journal::{Event, NamespaceAction, Step},
                exdev_source::{NamespacePath, NamespaceScope, plan_namespace_move},
            };
            for kind in [0, 1, 2, 3] {
                let f = Fixture::new();
                if kind == 1 {
                    fs::remove_file(f.paths[0].join("x")).unwrap();
                    fs::create_dir(f.paths[0].join("x")).unwrap();
                    fs::write(f.paths[0].join("x/a"), b"leaf").unwrap();
                    fs::hard_link(f.paths[0].join("x/a"), f.paths[0].join("x/b")).unwrap();
                    symlink("a", f.paths[0].join("x/link")).unwrap();
                    fs::write(
                        f.paths[0]
                            .join("x")
                            .join(OsString::from_vec(b"raw\xff\n".to_vec())),
                        vec![73u8; 131073],
                    )
                    .unwrap();
                }
                if kind == 3 {
                    fs::remove_file(f.paths[0].join("x")).unwrap();
                    symlink("missing-literal", f.paths[0].join("x")).unwrap();
                }
                own_old(&f, kind == 2);
                let attr = |path: &Path, name: &[u8], value: &[u8]| {
                    assert_eq!(
                        unsafe {
                            libc::setxattr(
                                c(path.as_os_str().as_bytes()).unwrap().as_ptr(),
                                c(name).unwrap().as_ptr(),
                                value.as_ptr().cast(),
                                value.len(),
                                0,
                            )
                        },
                        0
                    );
                };
                if kind == 0 {
                    attr(&f.paths[0].join("x"), b"user.source", b"source-only");
                    attr(&f.paths[1].join("x"), b"user.retained", b"basic-old-values");
                    let mut acl = 2u32.to_le_bytes().to_vec();
                    for (tag, perms, id) in [
                        (1u16, 7u16, u32::MAX),
                        (2, 4, unsafe { libc::geteuid() } + 1),
                        (4, 5, u32::MAX),
                        (16, 5, u32::MAX),
                        (32, 1, u32::MAX),
                    ] {
                        acl.extend_from_slice(&tag.to_le_bytes());
                        acl.extend_from_slice(&perms.to_le_bytes());
                        acl.extend_from_slice(&id.to_le_bytes());
                    }
                    attr(&f.paths[1].join("x"), b"system.posix_acl_access", &acl);
                }
                if kind == 1 {
                    attr(&f.paths[0].join("x"), b"user.full", b"directory-source");
                    attr(&f.paths[0].join("x/a"), b"user.hardlink", b"full-cohort");
                }
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                assert_eq!(physical.transactions().len(), 1);
                let store = create(&f, &physical, &prepared, [61; 16]);
                let (mut journal, mut workspace, parent) =
                    own_stage(&f, &physical, &store, &prepared);
                if kind == 0 {
                    let expected = StoredProof::decode(&store.proof().encode()).unwrap();
                    let v = store.value(&expected, Target::Stage(0)).unwrap();
                    assert!(
                        v.values()
                            .attributes()
                            .iter()
                            .any(|a| a.name() == b"user.retained")
                    );
                    assert!(
                        !v.values()
                            .attributes()
                            .iter()
                            .any(|a| a.name() == b"user.source")
                    );
                }
                let cancel = AtomicBool::new(false);
                let mut input = prepared
                    .namespace_snapshot(1, 1024 * 1024, &cancel)
                    .unwrap();
                let rename = plan_namespace_move(
                    vec![
                        NamespaceScope {
                            inventory: &mut input.inventory,
                            parent: &input.parent,
                            name: &input.name,
                        },
                        NamespaceScope {
                            inventory: &mut workspace,
                            parent: &parent,
                            name: std::ffi::OsStr::new("w"),
                        },
                    ],
                    NamespacePath {
                        scope: 0,
                        relative: b"x",
                    },
                    NamespacePath {
                        scope: 1,
                        relative: b"old",
                    },
                    1024 * 1024,
                    &cancel,
                )
                .unwrap();
                let result = journal
                    .execute_namespace(0, NamespaceAction::Preserve, rename, 1024 * 1024, &cancel)
                    .unwrap();
                assert_ne!(result.intent_seal(), [0; 32]);
                assert_eq!(result.action(), NamespaceAction::Preserve);
                assert_eq!(
                    journal.history().state.pending_step(),
                    Some((0, Step::Ready))
                );
                assert!(!f.paths[1].join("x").exists());
                assert!(f.base.join("w/old").exists());
                journal.append(Event::PublishIntent(0), &cancel).unwrap();
                let rename = plan_namespace_move(
                    vec![
                        NamespaceScope {
                            inventory: &mut workspace,
                            parent: &parent,
                            name: std::ffi::OsStr::new("w"),
                        },
                        NamespaceScope {
                            inventory: &mut input.inventory,
                            parent: &input.parent,
                            name: &input.name,
                        },
                    ],
                    NamespacePath {
                        scope: 0,
                        relative: b"new",
                    },
                    NamespacePath {
                        scope: 1,
                        relative: b"x",
                    },
                    1024 * 1024,
                    &cancel,
                )
                .unwrap();
                let published = journal
                    .execute_namespace(0, NamespaceAction::Publish, rename, 1024 * 1024, &cancel)
                    .unwrap();
                assert_eq!(published.transaction(), 0);
                assert!(published.sequence() > result.sequence());
                assert_eq!(
                    journal.history().state.pending_step(),
                    Some((0, Step::Synced))
                );
                input
                    .inventory
                    .verify_for_sync(&input.parent, &input.name, 1024 * 1024, &cancel)
                    .unwrap();
                journal.append(Event::Committed(0), &cancel).unwrap();
                journal.append(Event::Completed, &cancel).unwrap();
                drop(journal);
                let history = Journal::open(&store, &physical).unwrap();
                assert_eq!(history.durability(), State::History);
                assert!(history.ensure_current().is_err());
                if kind == 1 {
                    assert_eq!(fs::read(f.paths[1].join("x/a")).unwrap(), b"leaf");
                    assert_eq!(
                        fs::read_link(f.paths[1].join("x/link")).unwrap(),
                        Path::new("a")
                    );
                } else if kind == 3 {
                    assert_eq!(
                        fs::read_link(f.paths[1].join("x")).unwrap(),
                        Path::new("missing-literal")
                    );
                } else {
                    assert_eq!(fs::read(f.paths[1].join("x")).unwrap(), b"source");
                }
                if kind == 2 {
                    assert_eq!(fs::read(f.base.join("w/old/a")).unwrap(), b"old");
                } else {
                    assert_eq!(fs::read(f.base.join("w/old")).unwrap(), b"old");
                }
            }
        }
        #[test]
        fn owned_namespace_add_publishes_only_after_durable_intent() {
            use crate::{
                directory_sync_journal::{Event, NamespaceAction},
                exdev_source::{NamespacePath, NamespaceScope, plan_namespace_move},
            };
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [62; 16]);
            let (mut journal, mut workspace, parent) = own_stage(&f, &physical, &store, &prepared);
            let cancel = AtomicBool::new(false);
            let mut input = prepared
                .namespace_snapshot(1, 1024 * 1024, &cancel)
                .unwrap();
            journal.append(Event::AbsenceChecked(0), &cancel).unwrap();
            journal.append(Event::PublishIntent(0), &cancel).unwrap();
            let rename = plan_namespace_move(
                vec![
                    NamespaceScope {
                        inventory: &mut workspace,
                        parent: &parent,
                        name: std::ffi::OsStr::new("w"),
                    },
                    NamespaceScope {
                        inventory: &mut input.inventory,
                        parent: &input.parent,
                        name: &input.name,
                    },
                ],
                NamespacePath {
                    scope: 0,
                    relative: b"new",
                },
                NamespacePath {
                    scope: 1,
                    relative: b"x",
                },
                1024 * 1024,
                &cancel,
            )
            .unwrap();
            let result = journal
                .execute_namespace(0, NamespaceAction::Publish, rename, 1024 * 1024, &cancel)
                .unwrap();
            assert!(result.sequence() > 0);
            assert_eq!(fs::read(f.paths[1].join("x")).unwrap(), b"source");
        }
        #[test]
        fn owned_namespace_faults_poison_owner_preserve_objects_and_never_advance_snapshots() {
            use crate::{
                directory_sync_journal::NamespaceAction,
                exdev_source::{NamespacePath, NamespaceScope, OwnPoint, plan_namespace_move},
            };
            for point in [
                OwnPoint::IntentWritten,
                OwnPoint::IntentFileSynced,
                OwnPoint::IntentAcknowledged,
                OwnPoint::IntentDurable,
                OwnPoint::BeforeSyscall,
                OwnPoint::Dispatch,
                OwnPoint::RenameReturned,
                OwnPoint::Verified,
                OwnPoint::DataSynced,
                OwnPoint::Confirmed,
            ] {
                let f = Fixture::new();
                own_old(&f, false);
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [63; 16]);
                let (mut journal, mut workspace, parent) =
                    own_stage(&f, &physical, &store, &prepared);
                let cancel = AtomicBool::new(false);
                let mut input = prepared
                    .namespace_snapshot(1, 1024 * 1024, &cancel)
                    .unwrap();
                let before = format!("{:?}", input.inventory.entries());
                let rename = plan_namespace_move(
                    vec![
                        NamespaceScope {
                            inventory: &mut input.inventory,
                            parent: &input.parent,
                            name: &input.name,
                        },
                        NamespaceScope {
                            inventory: &mut workspace,
                            parent: &parent,
                            name: std::ffi::OsStr::new("w"),
                        },
                    ],
                    NamespacePath {
                        scope: 0,
                        relative: b"x",
                    },
                    NamespacePath {
                        scope: 1,
                        relative: b"old",
                    },
                    1024 * 1024,
                    &cancel,
                )
                .unwrap();
                let error = journal
                    .execute_namespace_with(
                        0,
                        NamespaceAction::Preserve,
                        rename,
                        1024 * 1024,
                        &cancel,
                        |p| {
                            if p == point {
                                Err(io::Error::other("lost callback/ACK"))
                            } else {
                                Ok(())
                            }
                        },
                    )
                    .unwrap_err();
                assert_eq!(
                    error.intent_recorded,
                    !matches!(
                        point,
                        OwnPoint::IntentWritten
                            | OwnPoint::IntentFileSynced
                            | OwnPoint::IntentAcknowledged
                    )
                );
                assert_eq!(
                    error.syscall_attempted,
                    !matches!(
                        point,
                        OwnPoint::IntentWritten
                            | OwnPoint::IntentFileSynced
                            | OwnPoint::IntentAcknowledged
                            | OwnPoint::IntentDurable
                            | OwnPoint::BeforeSyscall
                            | OwnPoint::Dispatch
                    )
                );
                assert_eq!(error.syscall_succeeded, error.syscall_attempted);
                assert_eq!(journal.durability(), State::Uncertain);
                assert!(journal.ensure_current().is_err());
                assert_eq!(format!("{:?}", input.inventory.entries()), before);
                let location = if error.syscall_succeeded {
                    f.base.join("w/old")
                } else {
                    f.paths[1].join("x")
                };
                assert_eq!(fs::read(location).unwrap(), b"old");
                assert_eq!(fs::read(f.base.join("w/new")).unwrap(), b"source");
                drop(journal);
                let history = Journal::open(&store, &physical).unwrap();
                assert_eq!(history.durability(), State::History);
            }
        }
        #[test]
        fn owned_namespace_race_collision_and_source_replacement_do_not_overwrite_or_retry() {
            use crate::{
                directory_sync_journal::NamespaceAction,
                exdev_source::{NamespacePath, NamespaceScope, OwnPoint, plan_namespace_move},
            };
            for (collision, late) in [(true, false), (false, false), (false, true)] {
                let f = Fixture::new();
                own_old(&f, false);
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [64; 16]);
                let (mut journal, mut workspace, parent) =
                    own_stage(&f, &physical, &store, &prepared);
                let cancel = AtomicBool::new(false);
                let mut input = prepared
                    .namespace_snapshot(1, 1024 * 1024, &cancel)
                    .unwrap();
                let rename = plan_namespace_move(
                    vec![
                        NamespaceScope {
                            inventory: &mut input.inventory,
                            parent: &input.parent,
                            name: &input.name,
                        },
                        NamespaceScope {
                            inventory: &mut workspace,
                            parent: &parent,
                            name: std::ffi::OsStr::new("w"),
                        },
                    ],
                    NamespacePath {
                        scope: 0,
                        relative: b"x",
                    },
                    NamespacePath {
                        scope: 1,
                        relative: b"old",
                    },
                    1024 * 1024,
                    &cancel,
                )
                .unwrap();
                let error = journal
                    .execute_namespace_with(
                        0,
                        NamespaceAction::Preserve,
                        rename,
                        1024 * 1024,
                        &cancel,
                        |p| {
                            if p == if late {
                                OwnPoint::Dispatch
                            } else {
                                OwnPoint::BeforeSyscall
                            } {
                                if collision {
                                    fs::write(f.base.join("w/old"), b"foreign")?;
                                } else {
                                    fs::rename(f.paths[1].join("x"), f.paths[1].join("kept"))?;
                                    fs::write(f.paths[1].join("x"), b"foreign")?;
                                }
                            }
                            Ok(())
                        },
                    )
                    .unwrap_err();
                assert_eq!(error.syscall_attempted, collision || late);
                assert_eq!(error.syscall_succeeded, late);
                assert_eq!(journal.durability(), State::Uncertain);
                if collision {
                    assert_eq!(fs::read(f.base.join("w/old")).unwrap(), b"foreign");
                    assert_eq!(fs::read(f.paths[1].join("x")).unwrap(), b"old");
                } else {
                    if late {
                        assert!(!f.paths[1].join("x").exists());
                        assert_eq!(fs::read(f.base.join("w/old")).unwrap(), b"foreign");
                    } else {
                        assert_eq!(fs::read(f.paths[1].join("x")).unwrap(), b"foreign");
                    }
                    assert_eq!(fs::read(f.paths[1].join("kept")).unwrap(), b"old");
                }
            }
        }
        #[test]
        fn owned_namespace_retirement_preserves_directory_without_publishing_new_data() {
            use crate::{
                directory_sync::{Mode, Side},
                directory_sync_journal::{Event, NamespaceAction},
                directory_sync_plan::Decision,
                exdev_source::{NamespacePath, NamespaceScope, plan_namespace_move},
            };
            let f = Fixture::new();
            fs::remove_file(f.paths[0].join("x")).unwrap();
            own_old(&f, true);
            let cancel = AtomicBool::new(false);
            let prepared = capture(
                Intent {
                    version: 1,
                    id: 44,
                    left: f.paths[0].clone(),
                    right: f.paths[1].clone(),
                    expected: None,
                    mode: Mode::Mirror(Side::Left),
                    read_limit: 1024 * 1024,
                },
                &cancel,
                |_| Ok(()),
            )
            .unwrap();
            let record = prepared.record().unwrap();
            let mut plan = Plan::new(&record, Options::default()).unwrap();
            for i in 0..plan.lines().len() {
                if plan.lines()[i].decision() == Decision::Pending {
                    let receipt = plan.receipt(i).unwrap();
                    plan.choose(receipt, Decision::Retire).unwrap();
                }
            }
            let physical = Physical::derive(&plan).unwrap();
            assert!(physical.nodes().is_empty());
            let store = create(&f, &physical, &prepared, [65; 16]);
            let (mut journal, mut workspace, parent) = own_stage(&f, &physical, &store, &prepared);
            let mut input = prepared
                .namespace_snapshot(1, 1024 * 1024, &cancel)
                .unwrap();
            let rename = plan_namespace_move(
                vec![
                    NamespaceScope {
                        inventory: &mut input.inventory,
                        parent: &input.parent,
                        name: &input.name,
                    },
                    NamespaceScope {
                        inventory: &mut workspace,
                        parent: &parent,
                        name: std::ffi::OsStr::new("w"),
                    },
                ],
                NamespacePath {
                    scope: 0,
                    relative: b"x",
                },
                NamespacePath {
                    scope: 1,
                    relative: b"old",
                },
                1024 * 1024,
                &cancel,
            )
            .unwrap();
            journal
                .execute_namespace(0, NamespaceAction::Preserve, rename, 1024 * 1024, &cancel)
                .unwrap();
            for e in [
                Event::EffectSynced(0),
                Event::Committed(0),
                Event::Completed,
            ] {
                journal.append(e, &cancel).unwrap();
            }
            assert!(!f.paths[1].join("x").exists());
            assert!(!f.base.join("w/new").exists());
            assert_eq!(fs::read(f.base.join("w/old/a")).unwrap(), b"old");
        }
        #[test]
        fn owned_namespace_rejects_wrong_role_path_and_changed_scope_before_recording_intent() {
            use crate::{
                directory_sync_journal::NamespaceAction,
                exdev_source::{
                    NamespacePath, NamespaceScope, capture_namespace, plan_namespace_move,
                },
            };
            for changed in [false, true] {
                let f = Fixture::new();
                own_old(&f, false);
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [66; 16]);
                let (mut journal, mut workspace, parent) =
                    own_stage(&f, &physical, &store, &prepared);
                let cancel = AtomicBool::new(false);
                if changed {
                    fs::write(f.paths[1].join("x"), b"bad").unwrap();
                }
                let mut input =
                    capture_namespace(&parent, std::ffi::OsStr::new("b"), 1024 * 1024, &cancel)
                        .unwrap();
                let before = fs::read(journal.path()).unwrap();
                let rename = plan_namespace_move(
                    vec![
                        NamespaceScope {
                            inventory: &mut input,
                            parent: &parent,
                            name: std::ffi::OsStr::new("b"),
                        },
                        NamespaceScope {
                            inventory: &mut workspace,
                            parent: &parent,
                            name: std::ffi::OsStr::new("w"),
                        },
                    ],
                    NamespacePath {
                        scope: 0,
                        relative: b"x",
                    },
                    NamespacePath {
                        scope: 1,
                        relative: if changed { b"old" } else { b"other" },
                    },
                    1024 * 1024,
                    &cancel,
                )
                .unwrap();
                let error = journal
                    .execute_namespace(0, NamespaceAction::Preserve, rename, 1024 * 1024, &cancel)
                    .unwrap_err();
                assert!(!error.intent_recorded);
                assert!(!error.syscall_attempted);
                assert_eq!(fs::read(journal.path()).unwrap(), before);
                journal.ensure_current().unwrap();
                assert!(f.paths[1].join("x").exists());
            }
        }
        #[test]
        fn owned_namespace_intent_codec_pending_history_cannot_implicitly_retry() {
            use crate::{
                directory_sync_journal::{Event, NamespaceAction, decode_event, encode_event},
                exdev_source::{NamespacePath, NamespaceScope, plan_namespace_move},
            };
            let event = Event::NamespaceIntent {
                transaction: 0,
                action: NamespaceAction::Preserve,
                seal: [7; 32],
            };
            let wire = encode_event(event);
            assert_eq!(wire.len(), 38);
            assert_eq!(decode_event(&wire).unwrap(), event);
            for n in 0..wire.len() {
                assert!(decode_event(&wire[..n]).is_err());
            }
            let mut bad = wire.clone();
            bad[5] = 2;
            assert!(decode_event(&bad).is_err());
            let f = Fixture::new();
            own_old(&f, false);
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = own_resolve(&record);
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [67; 16]);
            let (mut journal, mut workspace, parent) = own_stage(&f, &physical, &store, &prepared);
            let cancel = AtomicBool::new(false);
            journal.append(event, &cancel).unwrap();
            assert!(journal.append(event, &cancel).is_err());
            drop(journal);
            let mut history = Journal::open(&store, &physical).unwrap();
            assert!(history.ensure_current().is_err());
            history.renew(&cancel).unwrap();
            assert_eq!(
                history.history().state.namespace_guard(),
                Some((0, NamespaceAction::Preserve, [7; 32]))
            );
            let mut input = prepared
                .namespace_snapshot(1, 1024 * 1024, &cancel)
                .unwrap();
            let rename = plan_namespace_move(
                vec![
                    NamespaceScope {
                        inventory: &mut input.inventory,
                        parent: &input.parent,
                        name: &input.name,
                    },
                    NamespaceScope {
                        inventory: &mut workspace,
                        parent: &parent,
                        name: std::ffi::OsStr::new("w"),
                    },
                ],
                NamespacePath {
                    scope: 0,
                    relative: b"x",
                },
                NamespacePath {
                    scope: 1,
                    relative: b"old",
                },
                1024 * 1024,
                &cancel,
            )
            .unwrap();
            let error = history
                .execute_namespace(0, NamespaceAction::Preserve, rename, 1024 * 1024, &cancel)
                .unwrap_err();
            assert!(!error.syscall_attempted);
            assert_eq!(fs::read(f.paths[1].join("x")).unwrap(), b"old");
        }
        #[test]
        fn owned_namespace_cooperative_pause_cancel_and_resume_guard_actual_syscall() {
            use crate::{
                copy_engine::Control,
                directory_sync_journal::NamespaceAction,
                exdev_source::{NamespacePath, NamespaceScope, plan_namespace_move},
            };
            use std::{sync::mpsc, time::Duration};
            for cancelled in [false, true] {
                let f = Fixture::new();
                own_old(&f, false);
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [68; 16]);
                let (mut journal, mut workspace, parent) =
                    own_stage(&f, &physical, &store, &prepared);
                let flag = AtomicBool::new(false);
                let mut input = prepared.namespace_snapshot(1, 1024 * 1024, &flag).unwrap();
                let control = Control::default();
                control.pause();
                let worker_control = control.clone();
                let rename = plan_namespace_move(
                    vec![
                        NamespaceScope {
                            inventory: &mut input.inventory,
                            parent: &input.parent,
                            name: &input.name,
                        },
                        NamespaceScope {
                            inventory: &mut workspace,
                            parent: &parent,
                            name: std::ffi::OsStr::new("w"),
                        },
                    ],
                    NamespacePath {
                        scope: 0,
                        relative: b"x",
                    },
                    NamespacePath {
                        scope: 1,
                        relative: b"old",
                    },
                    1024 * 1024,
                    &flag,
                )
                .unwrap();
                std::thread::scope(|scope| {
                    let (entered_tx, entered_rx) = mpsc::channel();
                    let (done_tx, done_rx) = mpsc::channel();
                    let owner = &mut journal;
                    let worker = scope.spawn(move || {
                        entered_tx.send(()).unwrap();
                        let result = owner.execute_namespace_controlled(
                            0,
                            NamespaceAction::Preserve,
                            rename,
                            1024 * 1024,
                            &worker_control,
                        );
                        done_tx.send(result).unwrap();
                    });
                    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                    assert!(done_rx.recv_timeout(Duration::from_millis(30)).is_err());
                    assert!(f.paths[1].join("x").exists());
                    if cancelled {
                        control.cancel();
                    } else {
                        control.resume();
                    }
                    let result = done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                    worker.join().unwrap();
                    if cancelled {
                        let error = result.unwrap_err();
                        assert!(!error.intent_recorded);
                        assert!(!error.syscall_attempted);
                    } else {
                        assert!(result.is_ok());
                    }
                });
                if cancelled {
                    journal.ensure_current().unwrap();
                    assert!(f.paths[1].join("x").exists());
                } else {
                    assert!(!f.paths[1].join("x").exists());
                }
            }
        }
        #[test]
        fn owned_namespace_process_exit_after_syscall_sync_and_confirmation_reopens_history() {
            use crate::{
                directory_sync_journal::NamespaceAction,
                exdev_source::{NamespacePath, NamespaceScope, OwnPoint, plan_namespace_move},
            };
            const ENV: &str = "FILES_OWN_NAMESPACE_CHILD";
            if let Some(base) = std::env::var_os(ENV) {
                let base = PathBuf::from(base);
                let (mut store, saved) = Store::open_saved(&base.join("values"), [69; 16]).unwrap();
                let plan = saved.plan().unwrap();
                let physical = Physical::derive_with_limit(&plan, saved.stage_limit()).unwrap();
                let flag = AtomicBool::new(false);
                store.renew(&physical, &flag).unwrap();
                let mut journal = Journal::open(&store, &physical).unwrap();
                journal.renew(&flag).unwrap();
                let parent = File::open(&base).unwrap();
                let mut input = crate::exdev_source::capture_namespace(
                    &parent,
                    std::ffi::OsStr::new("b"),
                    1024 * 1024,
                    &flag,
                )
                .unwrap();
                let mut workspace = crate::exdev_source::capture_namespace(
                    &parent,
                    std::ffi::OsStr::new("w"),
                    1024 * 1024,
                    &flag,
                )
                .unwrap();
                let point = std::env::var("FILES_OWN_NAMESPACE_POINT").unwrap();
                let rename = plan_namespace_move(
                    vec![
                        NamespaceScope {
                            inventory: &mut input,
                            parent: &parent,
                            name: std::ffi::OsStr::new("b"),
                        },
                        NamespaceScope {
                            inventory: &mut workspace,
                            parent: &parent,
                            name: std::ffi::OsStr::new("w"),
                        },
                    ],
                    NamespacePath {
                        scope: 0,
                        relative: b"x",
                    },
                    NamespacePath {
                        scope: 1,
                        relative: b"old",
                    },
                    1024 * 1024,
                    &flag,
                )
                .unwrap();
                journal
                    .execute_namespace_with(
                        0,
                        NamespaceAction::Preserve,
                        rename,
                        1024 * 1024,
                        &flag,
                        |p| {
                            if matches!(
                                (point.as_str(), p),
                                ("rename", OwnPoint::RenameReturned)
                                    | ("data", OwnPoint::DataSynced)
                                    | ("confirmed", OwnPoint::Confirmed)
                            ) {
                                unsafe { libc::_exit(83) }
                            }
                            Ok(())
                        },
                    )
                    .unwrap();
                panic!("exit hook missing");
            }
            for point in ["rename", "data", "confirmed"] {
                let f = Fixture::new();
                own_old(&f, false);
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = own_resolve(&record);
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [69; 16]);
                let (journal, _, _) = own_stage(&f, &physical, &store, &prepared);
                drop(journal);
                drop(store);
                let status=Command::new(std::env::current_exe().unwrap()).args(["--exact","directory_sync_store::linux::tests::owned_namespace_process_exit_after_syscall_sync_and_confirmation_reopens_history","--nocapture"]).env(ENV,&f.base).env("FILES_OWN_NAMESPACE_POINT",point).status().unwrap();
                assert_eq!(status.code(), Some(83));
                assert!(!f.paths[1].join("x").exists());
                assert_eq!(fs::read(f.base.join("w/old")).unwrap(), b"old");
                let store = Store::open(&f.store, &physical, [69; 16]).unwrap();
                let history = Journal::open(&store, &physical).unwrap();
                assert_eq!(history.durability(), State::History);
                assert!(history.ensure_current().is_err());
                if point != "confirmed" {
                    assert!(history.history().state.namespace_guard().is_some());
                }
            }
        }
        #[test]
        fn effect_journal_readback_rejects_prior_record_edit_hidden_by_own_write() {
            use crate::directory_sync_journal::{Event, HEADER};
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [47; 16]);
            let cancel = AtomicBool::new(false);
            let mut journal = Journal::create(&store, &physical, &cancel).unwrap();
            journal
                .append(Event::Approval { ticket: [1; 16] }, &cancel)
                .unwrap();
            let path = journal.path();
            assert!(
                journal
                    .append_with(Event::WorkspaceIntent(0), &cancel, |point| {
                        if point == Barrier::Write {
                            let mut bytes = fs::read(&path)?;
                            bytes[HEADER + 44 + 1] ^= 2;
                            fs::write(&path, bytes)?;
                        }
                        Ok(())
                    })
                    .is_err()
            );
            assert_eq!(journal.durability(), State::Uncertain);
            assert_eq!(journal.history().sequence, 1);
            assert!(journal.renew(&cancel).is_err());
            drop(journal);
            let retained = fs::read(&path).unwrap();
            assert!(Journal::open(&store, &physical).is_err());
            assert_eq!(fs::read(path).unwrap(), retained);
        }
        #[test]
        fn effect_journal_empty_plan_and_cancelled_append_preserve_bytes() {
            use crate::directory_sync_journal::{Event, Phase};
            let f = Fixture::new();
            fs::remove_file(f.paths[0].join("x")).unwrap();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&plan).unwrap();
            assert!(physical.transactions().is_empty());
            assert!(physical.directory_metadata().is_empty());
            let store = create(&f, &physical, &prepared, [46; 16]);
            let cancel = AtomicBool::new(true);
            assert!(Journal::create(&store, &physical, &cancel).is_err());
            cancel.store(false, Ordering::Relaxed);
            let mut journal = Journal::create(&store, &physical, &cancel).unwrap();
            let before = fs::read(journal.path()).unwrap();
            cancel.store(true, Ordering::Relaxed);
            assert!(
                journal
                    .append(Event::Approval { ticket: [1; 16] }, &cancel)
                    .is_err()
            );
            assert_eq!(fs::read(journal.path()).unwrap(), before);
            journal.ensure_current().unwrap();
            cancel.store(false, Ordering::Relaxed);
            for event in [
                Event::Approval { ticket: [1; 16] },
                Event::StagingComplete,
                Event::Completed,
                Event::UndoBegin,
                Event::Undone,
            ] {
                journal.append(event, &cancel).unwrap();
            }
            assert_eq!(journal.history().state.phase(), Phase::Undone);
            drop(journal);
            let history = Journal::open(&store, &physical).unwrap();
            assert_eq!(history.history().state.phase(), Phase::Undone);
            assert_eq!(history.history().sequence, 5);
        }
        #[test]
        fn effect_journal_durable_append_history_and_explicit_renewal() {
            use crate::directory_sync_journal::{Event, Phase};
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [41; 16]);
            let cancel = AtomicBool::new(false);
            let mut journal = Journal::create(&store, &physical, &cancel).unwrap();
            let path = journal.path();
            let identity = journal.identity();
            assert!(Journal::create(&store, &physical, &cancel).is_err());
            assert!(Journal::open(&store, &physical).is_err());
            let before = fs::read(&path).unwrap();
            assert!(journal.append(Event::StagingComplete, &cancel).is_err());
            assert_eq!(fs::read(&path).unwrap(), before);
            journal
                .append(Event::Approval { ticket: [1; 16] }, &cancel)
                .unwrap();
            assert_eq!(journal.history().state.phase(), Phase::Staging);
            assert_eq!(journal.history().sequence, 1);
            journal.ensure_current().unwrap();
            drop(journal);
            let mut history = Journal::open(&store, &physical).unwrap();
            assert_eq!(history.identity(), identity);
            assert_eq!(history.durability(), State::History);
            assert!(history.ensure_current().is_err());
            assert!(history.append(Event::WorkspaceIntent(0), &cancel).is_err());
            history.renew(&cancel).unwrap();
            history.append(Event::WorkspaceIntent(0), &cancel).unwrap();
            assert_eq!(history.history().state.pending_transaction(), None);
            assert_eq!(history.history().sequence, 2);
            assert_eq!(fs::read(f.paths[0].join("x")).unwrap(), b"source");
            assert!(!f.paths[1].join("x").exists());
        }
        #[test]
        fn effect_journal_failed_ack_poison_and_torn_tail_never_truncated() {
            use crate::directory_sync_journal::Event;
            for point in [
                Barrier::Write,
                Barrier::ReadBack,
                Barrier::FileSynced,
                Barrier::DirectorySynced(0),
                Barrier::Acknowledged,
            ] {
                let f = Fixture::new();
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = Plan::new(&record, Options::default()).unwrap();
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [42; 16]);
                let cancel = AtomicBool::new(false);
                let mut journal = Journal::create(&store, &physical, &cancel).unwrap();
                let path = journal.path();
                assert!(
                    journal
                        .append_with(Event::Approval { ticket: [2; 16] }, &cancel, |p| {
                            if p == point {
                                Err(io::Error::other("lost ACK"))
                            } else {
                                Ok(())
                            }
                        })
                        .is_err()
                );
                assert_eq!(journal.durability(), State::Uncertain);
                assert_eq!(journal.history().sequence, 0);
                assert!(journal.renew(&cancel).is_err());
                assert!(
                    journal
                        .append(Event::Approval { ticket: [2; 16] }, &cancel)
                        .is_err()
                );
                drop(journal);
                let history = Journal::open(&store, &physical).unwrap();
                assert_eq!(history.history().sequence, 1);
                assert_eq!(history.durability(), State::History);
                drop(history);
                let mut damaged = fs::read(&path).unwrap();
                damaged.pop();
                fs::write(&path, &damaged).unwrap();
                assert!(Journal::open(&store, &physical).is_err());
                assert_eq!(fs::read(&path).unwrap(), damaged);
            }
        }
        #[test]
        fn effect_journal_process_exit_keeps_complete_claim_without_fresh_authority() {
            const CHILD: &str = "FILES_JOURNAL_EXIT_FIXTURE";
            if let Some(path) = std::env::var_os(CHILD) {
                use crate::directory_sync_journal::Event;
                let (mut store, saved) = Store::open_saved(Path::new(&path), [45; 16]).unwrap();
                let plan = saved.plan().unwrap();
                let physical = Physical::derive_with_limit(&plan, saved.stage_limit()).unwrap();
                let cancel = AtomicBool::new(false);
                store.renew(&physical, &cancel).unwrap();
                let mut journal = Journal::create(&store, &physical, &cancel).unwrap();
                let point = std::env::var("FILES_JOURNAL_EXIT_POINT").unwrap();
                journal
                    .append_with(Event::Approval { ticket: [4; 16] }, &cancel, |p| {
                        let hit = match point.as_str() {
                            "write" => p == Barrier::Write,
                            "sync" => p == Barrier::FileSynced,
                            "ack" => p == Barrier::Acknowledged,
                            _ => false,
                        };
                        if hit {
                            unsafe { libc::_exit(73) }
                        }
                        Ok(())
                    })
                    .unwrap();
                panic!("exit hook missing");
            }
            for point in ["write", "sync", "ack"] {
                let f = Fixture::new();
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let plan = Plan::new(&record, Options::default()).unwrap();
                let physical = Physical::derive(&plan).unwrap();
                let store = create(&f, &physical, &prepared, [45; 16]);
                drop(store);
                let status=Command::new(std::env::current_exe().unwrap()).args(["--exact","directory_sync_store::linux::tests::effect_journal_process_exit_keeps_complete_claim_without_fresh_authority","--nocapture"]).env(CHILD,&f.store).env("FILES_JOURNAL_EXIT_POINT",point).status().unwrap();
                assert_eq!(status.code(), Some(73));
                let mut store = Store::open(&f.store, &physical, [45; 16]).unwrap();
                let mut history = Journal::open(&store, &physical).unwrap();
                assert_eq!(history.history().sequence, 1);
                assert_eq!(history.durability(), State::History);
                assert!(history.renew(&AtomicBool::new(false)).is_err());
                drop(history);
                store.renew(&physical, &AtomicBool::new(false)).unwrap();
                let mut history = Journal::open(&store, &physical).unwrap();
                history.renew(&AtomicBool::new(false)).unwrap();
                history.ensure_current().unwrap();
                assert!(!f.paths[1].join("x").exists());
            }
        }
        #[test]
        fn effect_journal_rejects_aliases_replacements_and_foreign_preparation() {
            use crate::directory_sync_journal::Event;
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [43; 16]);
            let cancel = AtomicBool::new(false);
            let mut journal = Journal::create(&store, &physical, &cancel).unwrap();
            let path = journal.path();
            fs::hard_link(&path, f.store.join("alias")).unwrap();
            assert!(
                journal
                    .append(Event::Approval { ticket: [3; 16] }, &cancel)
                    .is_err()
            );
            fs::remove_file(f.store.join("alias")).unwrap();
            // ctime drift is deliberately not accepted by the existing owner.
            assert!(journal.ensure_current().is_err());
            drop(journal);
            let mut journal = Journal::open(&store, &physical).unwrap();
            journal.renew(&cancel).unwrap();
            let original_identity = journal.identity();
            let retained = f.store.join("retained-journal");
            fs::rename(&path, &retained).unwrap();
            fs::copy(&retained, &path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            assert!(journal.ensure_current().is_err());
            drop(journal);
            let replacement = Journal::open(&store, &physical).unwrap();
            assert!(replacement.require_identity(original_identity).is_err());
            drop(replacement);
            let other = create(&f, &physical, &prepared, [44; 16]);
            let foreign_name = f
                .store
                .join("sync-effects-2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c2c.journal");
            fs::copy(&path, &foreign_name).unwrap();
            fs::set_permissions(&foreign_name, fs::Permissions::from_mode(0o600)).unwrap();
            assert!(Journal::open(&other, &physical).is_err());
        }
        #[test]
        fn create_readback_namespace_sync_and_reopen_require_new_barriers() {
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let p = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&p).unwrap();
            let mut store = create(&f, &physical, &prepared, [1; 16]);
            assert_eq!(store.state(), State::Durable);
            let expected = StoredProof::decode(&store.proof().encode()).unwrap();
            store.barrier(&expected).unwrap().verify().unwrap();
            let m = fs::metadata(store.path()).unwrap();
            assert_eq!(m.mode() & 0o7777, 0o600);
            assert_eq!(m.nlink(), 1);
            let mut count = 0;
            store
                .inspect(&physical, |_, _, v| {
                    assert_eq!(v.kind(), Kind::File);
                    count += 1;
                    Ok(())
                })
                .unwrap();
            assert_eq!(count, 1);
            assert_eq!(
                Store::open(&f.store, &physical, [1; 16])
                    .err()
                    .unwrap()
                    .kind(),
                io::ErrorKind::WouldBlock
            );
            drop(store);
            let mut reopened = Store::open(&f.store, &physical, [1; 16]).unwrap();
            assert_eq!(reopened.state(), State::History);
            reopened.require_proof(&expected).unwrap();
            assert!(reopened.ensure_current().is_err());
            assert!(reopened.barrier(&expected).is_err());
            reopened.renew(&physical, &AtomicBool::new(false)).unwrap();
            reopened.barrier(&expected).unwrap().verify().unwrap();
        }
        #[test]
        fn exclusive_creation_never_clobbers_or_reuses_an_existing_name() {
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let p = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&p).unwrap();
            let store = create(&f, &physical, &prepared, [2; 16]);
            let original = fs::read(store.path()).unwrap();
            let error = Store::create(
                &f.store,
                &physical,
                &prepared,
                [2; 16],
                metadata::DEFAULT_STREAM_BYTES,
                &AtomicBool::new(false),
            )
            .err()
            .unwrap();
            assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
            assert_eq!(fs::read(store.path()).unwrap(), original);
            store.ensure_current().unwrap();
            assert_eq!(fs::read(f.paths[0].join("x")).unwrap(), b"source");
        }
        #[test]
        fn input_limits_foreign_capture_and_private_disjoint_namespace_checked_before_creation() {
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let p = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&p).unwrap();
            for limit in [0, metadata::MAX_STREAM_BYTES + 1] {
                assert!(
                    Store::create(
                        &f.store,
                        &physical,
                        &prepared,
                        [3; 16],
                        limit,
                        &AtomicBool::new(false)
                    )
                    .is_err()
                );
                assert!(!f.path([3; 16]).exists());
            }
            let other = Fixture::new();
            let foreign = other.prepare();
            assert!(
                Store::create(
                    &f.store,
                    &physical,
                    &foreign,
                    [3; 16],
                    metadata::DEFAULT_STREAM_BYTES,
                    &AtomicBool::new(false)
                )
                .is_err()
            );
            assert!(!f.path([3; 16]).exists());
            fs::set_permissions(&f.store, fs::Permissions::from_mode(0o755)).unwrap();
            assert!(
                Store::create(
                    &f.store,
                    &physical,
                    &prepared,
                    [3; 16],
                    metadata::DEFAULT_STREAM_BYTES,
                    &AtomicBool::new(false)
                )
                .is_err()
            );
            fs::set_permissions(&f.store, fs::Permissions::from_mode(0o700)).unwrap();
            symlink(&f.base, f.base.join("alias")).unwrap();
            assert!(
                Store::create(
                    &f.base.join("alias/values"),
                    &physical,
                    &prepared,
                    [3; 16],
                    metadata::DEFAULT_STREAM_BYTES,
                    &AtomicBool::new(false)
                )
                .is_err()
            );
            fs::create_dir(f.paths[0].join("cache")).unwrap();
            fs::set_permissions(f.paths[0].join("cache"), fs::Permissions::from_mode(0o700))
                .unwrap();
            let current = f.prepare();
            let r = current.record().unwrap();
            let plan = Plan::new(&r, Options::default()).unwrap();
            let phys = Physical::derive(&plan).unwrap();
            assert!(
                Store::create(
                    &f.paths[0].join("cache"),
                    &phys,
                    &current,
                    [3; 16],
                    metadata::DEFAULT_STREAM_BYTES,
                    &AtomicBool::new(false)
                )
                .is_err()
            );
            fs::set_permissions(&f.base, fs::Permissions::from_mode(0o700)).unwrap();
            assert!(
                Store::create(
                    &f.base,
                    &phys,
                    &current,
                    [3; 16],
                    metadata::DEFAULT_STREAM_BYTES,
                    &AtomicBool::new(false)
                )
                .is_err()
            );
            assert!(!f.path([3; 16]).exists());
        }
        #[test]
        fn partial_write_failure_preserves_unreusable_artifact_without_success_owner() {
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let p = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&p).unwrap();
            let mut writes = 0;
            let result = Store::create_with(
                &f.store,
                &physical,
                &prepared,
                [4; 16],
                metadata::DEFAULT_STREAM_BYTES,
                &AtomicBool::new(false),
                |b| {
                    if b == Barrier::Write {
                        writes += 1;
                        if writes == 2 {
                            return Err(io::Error::other("injected write"));
                        }
                    }
                    Ok(())
                },
            );
            assert!(result.is_err());
            assert_eq!(fs::metadata(f.path([4; 16])).unwrap().len(), 24);
            assert!(Store::open(&f.store, &physical, [4; 16]).is_err());
            assert_eq!(
                Store::create(
                    &f.store,
                    &physical,
                    &prepared,
                    [4; 16],
                    metadata::DEFAULT_STREAM_BYTES,
                    &AtomicBool::new(false)
                )
                .err()
                .unwrap()
                .kind(),
                io::ErrorKind::AlreadyExists
            );
        }
        #[test]
        fn failures_after_each_real_barrier_leave_history_without_fresh_durability() {
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let p = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&p).unwrap();
            for (i, point) in [
                Barrier::ValuesWritten,
                Barrier::ReadBack,
                Barrier::FileSynced,
                Barrier::DirectorySynced(0),
                Barrier::Acknowledged,
            ]
            .into_iter()
            .enumerate()
            {
                let job = [10 + i as u8; 16];
                assert!(
                    Store::create_with(
                        &f.store,
                        &physical,
                        &prepared,
                        job,
                        metadata::DEFAULT_STREAM_BYTES,
                        &AtomicBool::new(false),
                        |p| {
                            if p == point {
                                Err(io::Error::other("injected ACK"))
                            } else {
                                Ok(())
                            }
                        }
                    )
                    .is_err()
                );
                let mut history = Store::open(&f.store, &physical, job).unwrap();
                assert_eq!(history.state(), State::History);
                assert!(history.ensure_current().is_err());
                history.renew(&physical, &AtomicBool::new(false)).unwrap();
                history.ensure_current().unwrap();
            }
        }
        #[test]
        fn failed_renewal_latches_uncertain_until_drop_and_explicit_reopen() {
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let p = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&p).unwrap();
            let mut store = create(&f, &physical, &prepared, [5; 16]);
            let expected = StoredProof::decode(&store.proof().encode()).unwrap();
            assert!(
                store
                    .renew_with(&physical, &AtomicBool::new(false), |p| {
                        if p == Barrier::FileSynced {
                            Err(io::Error::other("ACK"))
                        } else {
                            Ok(())
                        }
                    })
                    .is_err()
            );
            assert_eq!(store.state(), State::Uncertain);
            assert!(store.ensure_current().is_err());
            assert!(store.renew(&physical, &AtomicBool::new(false)).is_err());
            assert!(store.barrier(&expected).is_err());
            drop(store);
            let mut reopened = Store::open(&f.store, &physical, [5; 16]).unwrap();
            reopened.require_proof(&expected).unwrap();
            reopened.renew(&physical, &AtomicBool::new(false)).unwrap();
            reopened.barrier(&expected).unwrap().verify().unwrap();
        }
        #[test]
        fn identical_replacement_bytes_do_not_recreate_original_file_identity() {
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let p = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&p).unwrap();
            let store = create(&f, &physical, &prepared, [6; 16]);
            let path = store.path();
            let original = store.proof().clone();
            let expected = StoredProof::decode(&original.encode()).unwrap();
            drop(store);
            let saved = f.store.join("original-kept");
            fs::rename(&path, &saved).unwrap();
            fs::copy(&saved, &path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            let mut replacement = Store::open(&f.store, &physical, [6; 16]).unwrap();
            assert_eq!(original.values_seal(), replacement.proof().values_seal());
            assert_ne!(
                original.file_identity(),
                replacement.proof().file_identity()
            );
            assert!(replacement.require_proof(&expected).is_err());
            replacement
                .renew(&physical, &AtomicBool::new(false))
                .unwrap();
            assert!(replacement.barrier(&expected).is_err());
        }
        #[test]
        fn live_hardlink_truncation_and_directory_substitution_revoke_binding() {
            for attack in 0..3 {
                let f = Fixture::new();
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let p = Plan::new(&record, Options::default()).unwrap();
                let physical = Physical::derive(&p).unwrap();
                let store = create(&f, &physical, &prepared, [7; 16]);
                let path = store.path();
                match attack {
                    0 => fs::hard_link(&path, f.base.join("alias-record")).unwrap(),
                    1 => fs::OpenOptions::new()
                        .write(true)
                        .open(&path)
                        .unwrap()
                        .set_len(12)
                        .unwrap(),
                    _ => {
                        fs::rename(&f.store, f.base.join("old-values")).unwrap();
                        fs::create_dir(&f.store).unwrap();
                        fs::set_permissions(&f.store, fs::Permissions::from_mode(0o700)).unwrap();
                    }
                }
                assert!(store.ensure_current().is_err());
            }
        }
        #[test]
        fn reopen_rejects_symlink_fifo_and_corrupted_private_records_before_read() {
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let p = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&p).unwrap();
            let outside = f.base.join("outside");
            fs::write(&outside, b"do-not-touch").unwrap();
            symlink(&outside, f.path([8; 16])).unwrap();
            assert!(Store::open(&f.store, &physical, [8; 16]).is_err());
            let fifo = c(f.path([9; 16]).as_os_str().as_bytes()).unwrap();
            assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
            assert!(Store::open(&f.store, &physical, [9; 16]).is_err());
            let store = create(&f, &physical, &prepared, [15; 16]);
            let path = store.path();
            drop(store);
            let mut bytes = fs::read(&path).unwrap();
            bytes[8] ^= 1;
            fs::write(&path, bytes).unwrap();
            assert!(Store::open(&f.store, &physical, [15; 16]).is_err());
            assert_eq!(fs::read(outside).unwrap(), b"do-not-touch");
        }
        #[test]
        fn final_ack_source_change_or_cancel_cannot_return_creation_receipt() {
            for cancel_at_end in [false, true] {
                let f = Fixture::new();
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let p = Plan::new(&record, Options::default()).unwrap();
                let physical = Physical::derive(&p).unwrap();
                let cancel = AtomicBool::new(false);
                assert!(
                    Store::create_with(
                        &f.store,
                        &physical,
                        &prepared,
                        [16; 16],
                        metadata::DEFAULT_STREAM_BYTES,
                        &cancel,
                        |point| {
                            if point == Barrier::Acknowledged {
                                if cancel_at_end {
                                    cancel.store(true, Ordering::Release)
                                } else {
                                    fs::write(f.paths[0].join("x"), b"changed")?;
                                }
                            }
                            Ok(())
                        }
                    )
                    .is_err()
                );
                let history = Store::open(&f.store, &physical, [16; 16]).unwrap();
                assert_eq!(history.state(), State::History);
            }
        }
        #[test]
        fn durable_artifact_renewal_never_implies_fresh_source_or_operator_approval() {
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let p = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&p).unwrap();
            let store = create(&f, &physical, &prepared, [17; 16]);
            let expected = StoredProof::decode(&store.proof().encode()).unwrap();
            drop(store);
            fs::write(f.paths[0].join("x"), b"changed").unwrap();
            let mut history = Store::open(&f.store, &physical, [17; 16]).unwrap();
            history.renew(&physical, &AtomicBool::new(false)).unwrap();
            history.barrier(&expected).unwrap().verify().unwrap();
            assert!(
                physical
                    .plan()
                    .bind(&prepared, &AtomicBool::new(false))
                    .is_err()
            );
        }
        #[test]
        fn proof_codec_is_comparison_only_and_checks_shape() {
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let p = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&p).unwrap();
            let store = create(&f, &physical, &prepared, [18; 16]);
            let bytes = store.proof().encode();
            assert!(StoredProof::decode(&bytes).unwrap().matches(store.proof()));
            for n in 0..bytes.len() {
                assert!(StoredProof::decode(&bytes[..n]).is_err());
            }
            for range in [0..8, 16..24, 32..40, 40..48, 48..56, 56..72, 200..208] {
                let mut changed = bytes;
                changed[range].fill(0);
                assert!(StoredProof::decode(&changed).is_err());
            }
            let mut changed = bytes;
            changed[168] ^= 1;
            let historic = StoredProof::decode(&changed).unwrap();
            assert!(!historic.matches(store.proof()));
            assert!(store.barrier(&historic).is_err());
        }
        #[test]
        fn abrupt_process_exit_preserves_history_or_partial_record_without_auto_resume() {
            if let Some(base) = std::env::var_os("FILES_SYNC_STORE_CRASH_ROOT") {
                let base = PathBuf::from(base);
                let f = Fixture {
                    paths: [base.join("a"), base.join("b")],
                    store: base.join("values"),
                    base,
                };
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let p = Plan::new(&record, Options::default()).unwrap();
                let physical = Physical::derive(&p).unwrap();
                let phase = std::env::var("FILES_SYNC_STORE_CRASH_PHASE").unwrap();
                let mut writes = 0;
                let _ = Store::create_with(
                    &f.store,
                    &physical,
                    &prepared,
                    [19; 16],
                    metadata::DEFAULT_STREAM_BYTES,
                    &AtomicBool::new(false),
                    |point| {
                        if point == Barrier::Write {
                            writes += 1;
                        }
                        if (phase == "partial" && point == Barrier::Write && writes == 2)
                            || (phase == "written" && point == Barrier::ValuesWritten)
                            || (phase == "ack" && point == Barrier::Acknowledged)
                        {
                            unsafe { libc::_exit(91) }
                        }
                        Ok(())
                    },
                );
                unsafe { libc::_exit(92) }
            }
            for phase in ["partial", "written", "ack"] {
                let f = Fixture::new();
                let status=Command::new(std::env::current_exe().unwrap()).args(["--exact","directory_sync_store::linux::tests::abrupt_process_exit_preserves_history_or_partial_record_without_auto_resume","--nocapture"]).env("FILES_SYNC_STORE_CRASH_ROOT",&f.base).env("FILES_SYNC_STORE_CRASH_PHASE",phase).status().unwrap();
                assert_eq!(status.code(), Some(91));
                for path in &f.paths {
                    fs::remove_dir_all(path).unwrap();
                }
                let reopened = Store::open_saved(&f.store, [19; 16]);
                if phase == "partial" {
                    assert!(reopened.is_err());
                    assert_eq!(fs::metadata(f.path([19; 16])).unwrap().len(), 24);
                } else {
                    let (mut store, saved) = reopened.unwrap();
                    let plan = saved.plan().unwrap();
                    let physical = Physical::derive_with_limit(&plan, saved.stage_limit()).unwrap();
                    assert_eq!(store.state(), State::History);
                    assert!(store.ensure_current().is_err());
                    store.renew(&physical, &AtomicBool::new(false)).unwrap();
                    store.ensure_current().unwrap();
                }
            }
        }
        #[test]
        fn saved_intent_recovers_decisions_and_stage_budget_without_original_paths_or_pins() {
            let f = Fixture::new();
            fs::write(f.paths[1].join("x"), b"old").unwrap();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let mut p = Plan::new(&record, Options::default()).unwrap();
            let row = (0..p.lines().len())
                .find(|&i| p.name(i) == Some(b"x".as_slice()))
                .unwrap();
            p.choose(
                p.receipt(row).unwrap(),
                crate::directory_sync_plan::Decision::Take(crate::directory_sync::Side::Right),
            )
            .unwrap();
            let physical = Physical::derive_with_limit(&p, 7).unwrap();
            let seal = physical.seal().unwrap();
            let store = create(&f, &physical, &prepared, [23; 16]);
            let expected = StoredProof::decode(&store.proof().encode()).unwrap();
            drop(store);
            drop(physical);
            drop(p);
            drop(record);
            drop(prepared);
            for path in &f.paths {
                fs::remove_dir_all(path).unwrap();
            }
            let (mut history, saved) = Store::open_saved(&f.store, [23; 16]).unwrap();
            assert_eq!(saved.stage_limit(), 7);
            let plan = saved.plan().unwrap();
            assert_eq!(
                plan.lines()[row].decision(),
                crate::directory_sync_plan::Decision::Take(crate::directory_sync::Side::Right)
            );
            let physical = Physical::derive_with_limit(&plan, saved.stage_limit()).unwrap();
            assert_eq!(seal, physical.seal().unwrap());
            history.require_proof(&expected).unwrap();
            let mut count = 0;
            history
                .inspect(&physical, |_, _, _| {
                    count += 1;
                    Ok(())
                })
                .unwrap();
            assert_eq!(count, 1);
            assert_eq!(history.state(), State::History);
        }
        #[test]
        fn deep_absolute_raw_store_path_is_not_confused_with_relative_content_depth() {
            let mut f = Fixture::new();
            let mut root = f.base.join("deep");
            for _ in 0..40 {
                root = root.join("n");
            }
            root = root.join(OsString::from_vec(b"raw\xff\n".to_vec()));
            fs::create_dir_all(&root).unwrap();
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
            f.store = root;
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let p = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&p).unwrap();
            let store = create(&f, &physical, &prepared, [21; 16]);
            store.ensure_current().unwrap();
            assert_eq!(store.path().parent(), Some(f.store.as_path()));
        }
        #[test]
        fn both_full_inventories_store_and_reopen_complete_values_beyond_eight_mib() {
            let f = Fixture::new();
            fs::remove_file(f.paths[0].join("x")).unwrap();
            for side in 0..2 {
                for i in 0..crate::directory_compare::MAX_ITEMS {
                    let path = f.paths[side].join(format!("{side}-{i:04}"));
                    fs::write(&path, b"").unwrap();
                    let name = c(path.as_os_str().as_bytes()).unwrap();
                    assert_eq!(
                        unsafe {
                            libc::setxattr(
                                name.as_ptr(),
                                c"user.store".as_ptr(),
                                [0x81u8; 320].as_ptr().cast(),
                                320,
                                0,
                            )
                        },
                        0
                    );
                }
            }
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let p = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&p).unwrap();
            let store = create(&f, &physical, &prepared, [22; 16]);
            let expected = StoredProof::decode(&store.proof().encode()).unwrap();
            assert!(store.held.file.metadata().unwrap().len() > 8 * 1024 * 1024);
            let path = store.path();
            drop(store);
            let mut reopened = Store::open(&f.store, &physical, [22; 16]).unwrap();
            assert_eq!(reopened.state(), State::History);
            reopened.require_proof(&expected).unwrap();
            let mut count = 0;
            reopened
                .inspect(&physical, |_, _, v| {
                    assert_eq!(v.attributes()[0].value(), [0x81u8; 320]);
                    count += 1;
                    Ok(())
                })
                .unwrap();
            assert_eq!(count, 16384);
            assert_eq!(reopened.index.retained_slots().0, 16384);
            assert!(reopened.index.retained_slots().1 <= metadata::MAX_RECORDS * 48);
            for i in [16383, 0, 8192, 8191] {
                assert_eq!(
                    reopened
                        .value(&expected, Target::Stage(i))
                        .unwrap()
                        .values()
                        .attributes()[0]
                        .value(),
                    [0x81u8; 320]
                );
            }
            assert_eq!(reopened.path(), path);
        }
        #[test]
        fn indexed_values_match_full_inspection_in_reverse_without_moving_file_cursor() {
            let f = Fixture::new();
            fs::create_dir_all(f.paths[0].join("tree/sub")).unwrap();
            fs::write(f.paths[0].join("tree/sub/file"), b"data").unwrap();
            symlink("missing", f.paths[0].join("link")).unwrap();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&plan).unwrap();
            let mut store = create(&f, &physical, &prepared, [31; 16]);
            let expected = StoredProof::decode(&store.proof().encode()).unwrap();
            let mut sequential = Vec::new();
            store
                .inspect(&physical, |target, flag, values| {
                    sequential.push((target, flag, values));
                    Ok(())
                })
                .unwrap();
            let before = store.held.file.stream_position().unwrap();
            for (target, flag, values) in sequential.into_iter().rev() {
                let item = store.value(&expected, target).unwrap();
                assert_eq!(item.target(), target);
                assert_eq!(item.after_children(), flag);
                assert_eq!(item.values(), &values);
            }
            assert_eq!(store.held.file.stream_position().unwrap(), before);
        }
        #[test]
        fn directory_desired_and_undo_slots_retain_distinct_flags_and_values() {
            let f = Fixture::new();
            fs::set_permissions(&f.paths[0], fs::Permissions::from_mode(0o700)).unwrap();
            fs::set_permissions(&f.paths[1], fs::Permissions::from_mode(0o755)).unwrap();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let mut plan = Plan::new(
                &record,
                Options {
                    root_metadata: true,
                    ..Options::default()
                },
            )
            .unwrap();
            let row = (0..plan.lines().len())
                .find(|&i| plan.name(i) == Some(b"".as_slice()))
                .unwrap();
            plan.choose(
                plan.receipt(row).unwrap(),
                crate::directory_sync_plan::Decision::Take(crate::directory_sync::Side::Left),
            )
            .unwrap();
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [32; 16]);
            let expected = StoredProof::decode(&store.proof().encode()).unwrap();
            let desired = store.value(&expected, Target::Directory(0)).unwrap();
            let undo = store.value(&expected, Target::UndoDirectory(0)).unwrap();
            assert_eq!(desired.values().mode(), 0o700);
            assert!(desired.after_children());
            assert_eq!(undo.values().mode(), 0o755);
            assert!(!undo.after_children());
            assert!(store.value(&expected, Target::Stage(0)).is_ok());
        }
        #[test]
        fn indexed_reads_are_bounded_to_one_record_and_invalid_targets_do_not_read() {
            use std::os::unix::fs::FileExt;
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [33; 16]);
            let mut calls = 0;
            let mut bytes = 0;
            let value = store
                .index
                .read_with(Target::Stage(0), |buffer, offset| {
                    calls += 1;
                    bytes += buffer.len();
                    store.held.file.read_exact_at(buffer, offset)
                })
                .unwrap();
            assert_eq!(calls, 1);
            assert_eq!(bytes, 10 + value.values().encode().unwrap().len());
            assert!(bytes <= 10 + metadata::MAX_VALUE_WIRE);
            for target in [
                Target::Stage(usize::MAX),
                Target::Stage(1),
                Target::Directory(0),
                Target::UndoDirectory(usize::MAX),
            ] {
                assert_eq!(
                    store
                        .index
                        .read_with(target, |_, _| panic!("invalid target read"))
                        .err()
                        .unwrap()
                        .kind(),
                    io::ErrorKind::InvalidInput
                );
            }
        }
        #[test]
        fn record_digest_detects_body_header_and_relocation_even_without_stat_guard() {
            use std::os::unix::fs::FileExt;
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [34; 16]);
            for offset in [0, 1, 5, 6, 10, 11, 30] {
                let result = store.index.read_with(Target::Stage(0), |buffer, at| {
                    store.held.file.read_exact_at(buffer, at)?;
                    buffer[offset] ^= 1;
                    Ok(())
                });
                assert!(result.is_err(), "byte {offset}");
            }
            assert!(
                store
                    .index
                    .read_with(Target::Stage(0), |buffer, at| store
                        .held
                        .file
                        .read_exact_at(buffer, at + 1))
                    .is_err()
            );
        }
        #[test]
        fn index_binds_expected_artifact_and_rechecks_after_record_read() {
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [35; 16]);
            let other = create(&f, &physical, &prepared, [36; 16]);
            let wrong = StoredProof::decode(&other.proof().encode()).unwrap();
            assert!(store.value(&wrong, Target::Stage(0)).is_err());
            let expected = StoredProof::decode(&store.proof().encode()).unwrap();
            assert!(
                store
                    .value_with(&expected, Target::Stage(0), || {
                        fs::hard_link(store.path(), f.base.join("unexpected-alias"))?;
                        Ok(())
                    })
                    .is_err()
            );
        }
        #[test]
        fn history_and_uncertain_index_reads_are_data_not_durable_or_source_authority() {
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [37; 16]);
            let expected = StoredProof::decode(&store.proof().encode()).unwrap();
            drop(store);
            let mut history = Store::open(&f.store, &physical, [37; 16]).unwrap();
            assert_eq!(history.state(), State::History);
            assert!(history.value(&expected, Target::Stage(0)).is_ok());
            assert!(history.barrier(&expected).is_err());
            assert!(
                history
                    .renew_with(&physical, &AtomicBool::new(false), |point| {
                        if point == Barrier::FileSynced {
                            Err(io::Error::other("ACK"))
                        } else {
                            Ok(())
                        }
                    })
                    .is_err()
            );
            assert_eq!(history.state(), State::Uncertain);
            assert!(history.value(&expected, Target::Stage(0)).is_ok());
            assert!(history.barrier(&expected).is_err());
        }
        #[test]
        fn concurrent_index_readers_are_independent_and_reopen_rebuilds_the_index() {
            let f = Fixture::new();
            for i in 0..12 {
                fs::write(f.paths[0].join(format!("f{i}")), b"x").unwrap();
            }
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [38; 16]);
            let expected = StoredProof::decode(&store.proof().encode()).unwrap();
            std::thread::scope(|scope| {
                for reverse in [false, true] {
                    let store = &store;
                    let expected = &expected;
                    scope.spawn(move || {
                        for i in 0..13 {
                            let target = Target::Stage(if reverse { 12 - i } else { i });
                            assert_eq!(store.value(expected, target).unwrap().target(), target);
                        }
                    });
                }
            });
            drop(store);
            let (history, saved) = Store::open_saved(&f.store, [38; 16]).unwrap();
            drop(saved);
            assert_eq!(history.index.retained_slots().0, 13);
            assert!(history.value(&expected, Target::Stage(12)).is_ok());
        }
        #[test]
        fn missing_original_inputs_do_not_prevent_indexed_historical_values() {
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&plan).unwrap();
            let store = create(&f, &physical, &prepared, [39; 16]);
            let expected = StoredProof::decode(&store.proof().encode()).unwrap();
            drop(store);
            drop(physical);
            drop(plan);
            drop(record);
            drop(prepared);
            for path in &f.paths {
                fs::remove_dir_all(path).unwrap();
            }
            let (history, saved) = Store::open_saved(&f.store, [39; 16]).unwrap();
            assert!(saved.plan().is_ok());
            assert_eq!(
                history
                    .value(&expected, Target::Stage(0))
                    .unwrap()
                    .values()
                    .kind(),
                Kind::File
            );
        }
        #[test]
        fn stage_metadata_order_has_unique_cohort_leaders_and_directory_postorder() {
            let f = Fixture::new();
            fs::create_dir_all(f.paths[0].join("tree/sub/deep")).unwrap();
            fs::write(f.paths[0].join("tree/sub/deep/a"), b"x").unwrap();
            fs::hard_link(
                f.paths[0].join("tree/sub/deep/a"),
                f.paths[0].join("outside-link"),
            )
            .unwrap();
            symlink("missing", f.paths[0].join("tree/sub/link")).unwrap();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&plan).unwrap();
            let order = physical.stage_metadata_order().unwrap();
            let view = record.view().unwrap();
            let names = order
                .iter()
                .map(|&i| {
                    let l = physical.nodes()[i].source();
                    view.roots[l.side].entries[l.index].relative
                })
                .collect::<Vec<_>>();
            assert_eq!(
                &names[names.len() - 3..],
                &[b"tree/sub/deep".as_slice(), b"tree/sub", b"tree"]
            );
            assert_eq!(order.len(), physical.nodes().len() - 1);
            let independent = Plan::new(
                &record,
                Options {
                    hardlinks: crate::directory_sync_plan::Hardlinks::IndependentNames,
                    ..Options::default()
                },
            )
            .unwrap();
            let separate = Physical::derive(&independent).unwrap();
            assert_eq!(
                separate.stage_metadata_order().unwrap().len(),
                separate.nodes().len()
            );
            let store = create(&f, &physical, &prepared, [40; 16]);
            let expected = StoredProof::decode(&store.proof().encode()).unwrap();
            for i in order {
                assert_eq!(
                    store.value(&expected, Target::Stage(i)).unwrap().target(),
                    Target::Stage(i)
                );
            }
        }
        #[test]
        fn indexed_value_digest_does_not_depend_on_a_claimed_file_timestamp() {
            use std::os::unix::fs::FileExt;
            let f = Fixture::new();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&plan).unwrap();
            let mut store = create(&f, &physical, &prepared, [41; 16]);
            let expected = StoredProof::decode(&store.proof().encode()).unwrap();
            let mut start = 0;
            store
                .index
                .read_with(Target::Stage(0), |buffer, offset| {
                    start = offset;
                    store.held.file.read_exact_at(buffer, offset)
                })
                .unwrap();
            let mut byte = [0];
            store
                .held
                .file
                .read_exact_at(&mut byte, start + 11)
                .unwrap();
            byte[0] ^= 1;
            store.held.file.write_all_at(&byte, start + 11).unwrap();
            // Controlled stand-in for an unavailable/coarse timestamp detector:
            // the independent original-record checksum must still reject data.
            store.stamp = stamp(&store.held.file.metadata().unwrap());
            assert!(
                store
                    .value(&expected, Target::Stage(0))
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("indexado")
            );
        }
        #[test]
        fn empty_preparation_has_an_empty_index_and_no_lookup_side_effects() {
            let f = Fixture::new();
            fs::remove_file(f.paths[0].join("x")).unwrap();
            let prepared = f.prepare();
            let record = prepared.record().unwrap();
            let plan = Plan::new(&record, Options::default()).unwrap();
            let physical = Physical::derive(&plan).unwrap();
            assert!(physical.stage_metadata_order().unwrap().is_empty());
            let store = create(&f, &physical, &prepared, [42; 16]);
            let expected = StoredProof::decode(&store.proof().encode()).unwrap();
            assert_eq!(store.index.retained_slots(), (0, 0));
            assert!(store.value(&expected, Target::Stage(0)).is_err());
            assert!(
                store
                    .index
                    .read_with(Target::Directory(0), |_, _| panic!("empty index read"))
                    .is_err()
            );
            drop(store);
            let (history, _) = Store::open_saved(&f.store, [42; 16]).unwrap();
            assert_eq!(history.index.retained_slots(), (0, 0));
        }
        #[test]
        fn creator_pid_guard_handles_child_drop_and_inherited_fd_after_parent_drop() {
            struct Child {
                pid: libc::pid_t,
                release: File,
            }
            impl Drop for Child {
                fn drop(&mut self) {
                    let _ = self.release.write_all(b"x");
                    let mut status = 0;
                    loop {
                        if unsafe { libc::waitpid(self.pid, &mut status, 0) } >= 0
                            || io::Error::last_os_error().kind() != io::ErrorKind::Interrupted
                        {
                            break;
                        }
                    }
                }
            }
            for drop_child in [false, true] {
                let f = Fixture::new();
                let prepared = f.prepare();
                let record = prepared.record().unwrap();
                let p = Plan::new(&record, Options::default()).unwrap();
                let physical = Physical::derive(&p).unwrap();
                let mut store = create(&f, &physical, &prepared, [20; 16]);
                let mut ready = [0; 2];
                let mut release = [0; 2];
                assert_eq!(
                    unsafe { libc::pipe2(ready.as_mut_ptr(), libc::O_CLOEXEC) },
                    0
                );
                assert_eq!(
                    unsafe { libc::pipe2(release.as_mut_ptr(), libc::O_CLOEXEC) },
                    0
                );
                let pid = unsafe { libc::fork() };
                assert!(pid >= 0);
                if pid == 0 {
                    unsafe {
                        libc::close(ready[0]);
                        libc::close(release[1]);
                        if drop_child {
                            std::ptr::drop_in_place(&mut store.held);
                        }
                        loop {
                            let n = libc::write(ready[1], b"r".as_ptr().cast(), 1);
                            if n == 1 {
                                break;
                            }
                            if n < 0 && *libc::__errno_location() == libc::EINTR {
                                continue;
                            }
                            libc::_exit(93);
                        }
                        let mut b = [0u8];
                        loop {
                            let n = libc::read(release[0], b.as_mut_ptr().cast(), 1);
                            if n == 1 {
                                break;
                            }
                            if n < 0 && *libc::__errno_location() == libc::EINTR {
                                continue;
                            }
                            libc::_exit(94);
                        }
                        libc::_exit(0)
                    }
                }
                unsafe {
                    libc::close(ready[1]);
                    libc::close(release[0]);
                }
                let _child = Child {
                    pid,
                    release: unsafe { File::from_raw_fd(release[1]) },
                };
                let mut input = unsafe { File::from_raw_fd(ready[0]) };
                let mut b = [0];
                input.read_exact(&mut b).unwrap();
                assert_eq!(
                    Store::open(&f.store, &physical, [20; 16])
                        .err()
                        .unwrap()
                        .kind(),
                    io::ErrorKind::WouldBlock
                );
                drop(store);
                let mut status = 0;
                loop {
                    let observed = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
                    if observed < 0
                        && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted
                    {
                        continue;
                    }
                    assert_eq!(
                        observed, 0,
                        "owned fork child must still be alive at parent unlock"
                    );
                    break;
                }
                let new_owner = Store::open(&f.store, &physical, [20; 16]).unwrap();
                assert_eq!(new_owner.state(), State::History);
            }
        }
    }
}
#[cfg(target_os = "linux")]
pub use linux::{
    BarrierReceipt, CreatedStageNode, CreatedWorkspace, CreationEvidence, Journal,
    JournalMetadataFailure, MAX_BATCH_METADATA_BYTES, MAX_PLACEMENT_BYTES, OwnNamespaceRename,
    OwnStageMetadata, Placement, PlacementRegistry, SavedIntent, StageBatchFailure,
    StageBatchPhase, StageBatchPoint, StageBatchReport, StageCreateFailure, StagedBatch,
    StagingReceipt, StagingWorkspace, State, Store, ValueRecord, WorkspaceBatchReport,
    WorkspaceFailure, WorkspaceLocation, WorkspaceParent,
};
