//! EXDEV write-ahead state and Linux journal store. This module never deletes,
//! renames or claims source/destination files. Executor observations must be
//! independently verified; a parseable log is not a fresh durability barrier.
use crate::move_manifest::{
    Identity, Kind, Manifest, Reader, Revision, put_identity, put_revision,
};
use std::{collections::HashMap, io};
/// Binds a reviewed operation to its directory/file identities and exact
/// manifest intent. It is NOT a fresh sync barrier or authorization to unlink.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntentProof {
    directory: Identity,
    journal: Identity,
    manifest_sha256: [u8; 32],
}
pub const MAX_SINGLE_FILE_JOURNAL: usize = 64 * 1024;
pub const MAX_JOURNAL: usize = 8 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"FLMJNL\0\x01";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Prepared,
    StageIntent,
    SourceStaged,
    PublishIntent,
    DestinationPublished,
    DestinationSynced,
    Deleting,
    Completed,
    RestoreIntent,
    Restored,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    StageIntent {
        source_stage: Identity,
        destination_stage: Identity,
    },
    SourceStaged {
        root_revision: Revision,
    },
    EntryVerified {
        index: u32,
        destination: Identity,
        content: [u8; 32],
        metadata: [u8; 32],
    },
    PublishIntent,
    DestinationPublished {
        root_revision: Revision,
    },
    DestinationSynced,
    DeleteIntent(u32),
    Removed(u32),
    Completed,
    RestoreIntent,
    Restored,
}
#[derive(Debug)]
pub struct State {
    phase: Phase,
    verified: usize,
    removed: usize,
    pending_delete: Option<u32>,
    source_stage: Option<Identity>,
    destination_stage: Option<Identity>,
    destinations: Vec<Identity>,
    source_revision: Option<Revision>,
    destination_revision: Option<Revision>,
    source_groups: HashMap<(u64, u64), Identity>,
    destination_groups: HashMap<(u64, u64), (u64, u64)>,
    // Deliberately NOT serialized. Lost on every open, including same-boot open.
    fresh_barriers: bool,
    checked_entry: Option<u32>,
    restore_checked: bool,
}
impl State {
    fn new(manifest: &Manifest) -> Result<Self, &'static str> {
        if manifest.root_count() != 1 {
            return Err("Cada journal/State requiere exactamente una raíz");
        }
        let count = manifest.entries().len();
        Ok(Self {
            phase: Phase::Prepared,
            verified: 0,
            removed: 0,
            pending_delete: None,
            source_stage: None,
            destination_stage: None,
            destinations: Vec::with_capacity(count),
            source_revision: None,
            destination_revision: None,
            source_groups: HashMap::with_capacity(count),
            destination_groups: HashMap::with_capacity(count),
            fresh_barriers: false,
            checked_entry: None,
            restore_checked: false,
        })
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn verified(&self) -> usize {
        self.verified
    }
    pub fn removed(&self) -> usize {
        self.removed
    }
    pub fn pending_delete(&self) -> Option<u32> {
        self.pending_delete
    }
    pub fn source_revision(&self) -> Option<Revision> {
        self.source_revision
    }
    pub fn destination_revision(&self) -> Option<Revision> {
        self.destination_revision
    }
    pub fn destinations(&self) -> &[Identity] {
        &self.destinations
    }
    pub fn source_stage(&self) -> Option<Identity> {
        self.source_stage
    }
    pub fn destination_stage(&self) -> Option<Identity> {
        self.destination_stage
    }
    pub fn barriers_current(&self) -> bool {
        self.fresh_barriers
    }
    fn deletion_index(&self, m: &Manifest) -> Option<u32> {
        m.entries()
            .len()
            .checked_sub(self.removed + 1)
            .map(|n| n as u32)
    }
    fn check(&self, m: &Manifest, e: Event, replay: bool) -> Result<(), &'static str> {
        let valid = match e {
            Event::StageIntent {
                source_stage: s,
                destination_stage: d,
            } => {
                self.phase == Phase::Prepared
                    && s.kind == Kind::Directory
                    && d.kind == Kind::Directory
                    && s.inode != 0
                    && d.inode != 0
                    && s.device == m.source_identity().device
                    && d.device == m.destination_identity().device
                    && s != m.source_identity()
                    && d != m.destination_identity()
                    && s != d
            }
            Event::SourceStaged { root_revision } => {
                self.phase == Phase::StageIntent
                    && root_revision.matches_after_rename(m.entries()[0].revision)
            }
            Event::EntryVerified {
                index,
                destination,
                content,
                metadata,
            } => {
                let Some(entry) = m.entries().get(index as usize) else {
                    return Err("Entrada fuera del manifiesto");
                };
                let source = (entry.identity.device, entry.identity.inode);
                let dest = (destination.device, destination.inode);
                self.phase == Phase::SourceStaged
                    && index as usize == self.verified
                    && destination.inode != 0
                    && destination.kind == entry.identity.kind
                    && destination.device == m.destination_identity().device
                    && Some(destination) != self.destination_stage
                    && destination != m.destination_identity()
                    && content == entry.content_digest
                    && metadata == entry.metadata_digest
                    && self
                        .source_groups
                        .get(&source)
                        .is_none_or(|old| *old == destination)
                    && self
                        .destination_groups
                        .get(&dest)
                        .is_none_or(|old| *old == source)
            }
            Event::PublishIntent => {
                self.phase == Phase::SourceStaged && self.verified == m.entries().len()
            }
            Event::DestinationPublished { root_revision } => {
                let expected = m.entries()[0].revision;
                let mut comparison = root_revision;
                if m.entries()[0].identity.kind == Kind::Directory {
                    comparison.size = expected.size;
                    comparison.links = expected.links;
                }
                self.phase == Phase::PublishIntent && comparison.matches_after_rename(expected)
            }
            Event::DestinationSynced => self.phase == Phase::DestinationPublished,
            Event::DeleteIntent(index) => {
                matches!(self.phase, Phase::DestinationSynced | Phase::Deleting)
                    && self.pending_delete.is_none()
                    && self.deletion_index(m) == Some(index)
                    && (replay || (self.fresh_barriers && self.checked_entry == Some(index)))
            }
            Event::Removed(index) => {
                self.phase == Phase::Deleting
                    && self.pending_delete == Some(index)
                    && (replay || (self.fresh_barriers && self.checked_entry == Some(index)))
            }
            Event::Completed => {
                self.phase == Phase::Deleting
                    && self.pending_delete.is_none()
                    && self.removed == m.entries().len()
                    && (replay || self.fresh_barriers)
            }
            Event::RestoreIntent => {
                self.removed == 0
                    && (replay || self.restore_checked)
                    && !matches!(
                        self.phase,
                        Phase::Completed | Phase::RestoreIntent | Phase::Restored
                    )
            }
            Event::Restored => self.phase == Phase::RestoreIntent,
        };
        if valid {
            Ok(())
        } else {
            Err("Transición de movimiento no autorizada o barrera actual ausente")
        }
    }
    fn commit(&mut self, m: &Manifest, e: Event) {
        match e {
            Event::StageIntent {
                source_stage,
                destination_stage,
            } => {
                self.source_stage = Some(source_stage);
                self.destination_stage = Some(destination_stage);
                self.phase = Phase::StageIntent;
            }
            Event::SourceStaged { root_revision } => {
                self.source_revision = Some(root_revision);
                self.phase = Phase::SourceStaged;
            }
            Event::EntryVerified {
                index, destination, ..
            } => {
                let id = m.entries()[index as usize].identity;
                let source = (id.device, id.inode);
                self.source_groups.insert(source, destination);
                self.destination_groups
                    .insert((destination.device, destination.inode), source);
                self.destinations.push(destination);
                self.verified += 1;
            }
            Event::PublishIntent => self.phase = Phase::PublishIntent,
            Event::DestinationPublished { root_revision } => {
                self.destination_revision = Some(root_revision);
                self.phase = Phase::DestinationPublished;
            }
            Event::DestinationSynced => {
                self.phase = Phase::DestinationSynced;
                self.fresh_barriers = false;
                self.checked_entry = None;
            }
            Event::DeleteIntent(index) => {
                self.phase = Phase::Deleting;
                self.pending_delete = Some(index);
            }
            Event::Removed(_) => {
                self.pending_delete = None;
                self.removed += 1;
                self.checked_entry = None;
            }
            Event::Completed => {
                self.phase = Phase::Completed;
                self.fresh_barriers = false;
                self.checked_entry = None;
            }
            Event::RestoreIntent => {
                self.pending_delete = None;
                self.restore_checked = false;
                self.phase = Phase::RestoreIntent;
                self.fresh_barriers = false;
                self.checked_entry = None;
            }
            Event::Restored => self.phase = Phase::Restored,
        }
    }
}
fn invalid(s: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, s.into())
}
fn crc(bytes: &[u8]) -> u32 {
    let mut value = !0u32;
    for b in bytes {
        value ^= *b as u32;
        for _ in 0..8 {
            value = (value >> 1) ^ (0xedb88320u32 & 0u32.wrapping_sub(value & 1));
        }
    }
    !value
}
fn encode_event(e: Event) -> Vec<u8> {
    let mut out = Vec::with_capacity(96);
    match e {
        Event::StageIntent {
            source_stage,
            destination_stage,
        } => {
            out.push(1);
            put_identity(&mut out, source_stage);
            put_identity(&mut out, destination_stage);
        }
        Event::SourceStaged { root_revision } => {
            out.push(2);
            put_revision(&mut out, root_revision);
        }
        Event::EntryVerified {
            index,
            destination,
            content,
            metadata,
        } => {
            out.push(3);
            out.extend_from_slice(&index.to_le_bytes());
            put_identity(&mut out, destination);
            out.extend_from_slice(&content);
            out.extend_from_slice(&metadata);
        }
        Event::PublishIntent => out.push(4),
        Event::DestinationPublished { root_revision } => {
            out.push(5);
            put_revision(&mut out, root_revision);
        }
        Event::DestinationSynced => out.push(6),
        Event::DeleteIntent(i) => {
            out.push(7);
            out.extend_from_slice(&i.to_le_bytes());
        }
        Event::Removed(i) => {
            out.push(8);
            out.extend_from_slice(&i.to_le_bytes());
        }
        Event::Completed => out.push(9),
        Event::RestoreIntent => out.push(10),
        Event::Restored => out.push(11),
    }
    out
}
fn decode_event(data: &[u8]) -> Result<Event, &'static str> {
    let mut r = Reader { data, at: 0 };
    let e = match r.take(1)?[0] {
        1 => Event::StageIntent {
            source_stage: r.identity()?,
            destination_stage: r.identity()?,
        },
        2 => Event::SourceStaged {
            root_revision: r.revision()?,
        },
        3 => Event::EntryVerified {
            index: r.u32()?,
            destination: r.identity()?,
            content: r.array()?,
            metadata: r.array()?,
        },
        4 => Event::PublishIntent,
        5 => Event::DestinationPublished {
            root_revision: r.revision()?,
        },
        6 => Event::DestinationSynced,
        7 => Event::DeleteIntent(r.u32()?),
        8 => Event::Removed(r.u32()?),
        9 => Event::Completed,
        10 => Event::RestoreIntent,
        11 => Event::Restored,
        _ => return Err("Evento journal desconocido"),
    };
    if r.at != data.len() {
        return Err("Evento journal con bytes sobrantes");
    }
    Ok(e)
}
fn initial(m: &Manifest) -> io::Result<Vec<u8>> {
    if m.root_count() != 1 {
        return Err(invalid("Cada journal requiere exactamente una raíz"));
    }
    let wire = m.encode().map_err(invalid)?;
    let mut out = Vec::with_capacity(wire.len() + 16);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(wire.len() as u32).to_le_bytes());
    out.extend_from_slice(&wire);
    let checksum = crc(&out);
    out.extend_from_slice(&checksum.to_le_bytes());
    Ok(out)
}
fn replay(data: &[u8]) -> io::Result<(Manifest, State, u64)> {
    if data.len() > MAX_JOURNAL {
        return Err(invalid("Journal excede límite"));
    }
    let mut r = Reader { data, at: 0 };
    if r.take(8).map_err(invalid)? != MAGIC {
        return Err(invalid("Journal incompatible preservado"));
    }
    let n = r.u32().map_err(invalid)? as usize;
    if n > crate::move_manifest::MAX_WIRE {
        return Err(invalid("Manifiesto journal excede límite"));
    }
    let manifest_bytes = r.take(n).map_err(invalid)?;
    let check_at = r.at;
    let expected = r.u32().map_err(invalid)?;
    if crc(&data[..check_at]) != expected {
        return Err(invalid("CRC de cabecera inválido"));
    }
    let manifest = Manifest::decode(manifest_bytes).map_err(invalid)?;
    let mut state = State::new(&manifest).map_err(invalid)?;
    let mut sequence = 0;
    while r.at < data.len() {
        let n = r.u32().map_err(invalid)? as usize;
        if !(9..=104).contains(&n) {
            return Err(invalid("Longitud de evento inválida"));
        }
        let payload = r.take(n).map_err(invalid)?;
        let expected = r.u32().map_err(invalid)?;
        if crc(payload) != expected {
            return Err(invalid("Cola journal corrupta preservada"));
        }
        let mut p = Reader {
            data: payload,
            at: 0,
        };
        let seq = p.u64().map_err(invalid)?;
        if seq != sequence + 1 || seq > (3 * manifest.entries().len() + 16) as u64 {
            return Err(invalid("Secuencia journal inválida"));
        }
        let e = decode_event(p.take(n - 8).map_err(invalid)?).map_err(invalid)?;
        state.check(&manifest, e, true).map_err(invalid)?;
        state.commit(&manifest, e);
        sequence = seq;
    }
    // Disk events can never renew volatile barriers, including a fully readable
    // DestinationSynced whose live fsync ACK failed.
    state.fresh_barriers = false;
    state.checked_entry = None;
    state.restore_checked = false;
    Ok((manifest, state, sequence))
}
#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::{
        ffi::CString,
        fs::File,
        io::{Read, Seek, SeekFrom, Write},
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{ffi::OsStrExt, fs::MetadataExt},
        },
        path::Path,
    };
    pub struct Journal {
        owner_pid: libc::pid_t,
        directory: File,
        file: File,
        name: CString,
        manifest: Manifest,
        state: State,
        sequence: u64,
        length: u64,
        uncertain: bool,
        revision: (i64, i64, i64, i64),
    }
    impl Drop for Journal {
        fn drop(&mut self) {
            // CLOEXEC closes inherited descriptors only at exec. Another thread
            // may be between fork/exec when this owner transfers to Recovery.
            // Release the advisory ownership explicitly on the creating process;
            // a forked copy must never unlock the still-live parent's owner.
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
    fn owned(fd: i32) -> io::Result<File> {
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }
    fn directory(path: &Path) -> io::Result<File> {
        let name = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| invalid("NUL en ruta journal"))?;
        let d = owned(unsafe {
            libc::open(
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })?;
        let m = d.metadata()?;
        if m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o077 != 0 {
            return Err(invalid("Journal requiere directorio privado del usuario"));
        }
        Ok(d)
    }
    fn name(job: [u8; 16]) -> CString {
        let mut s = String::from("move-");
        for b in job {
            use std::fmt::Write;
            write!(s, "{b:02x}").unwrap();
        }
        s.push_str(".journal");
        CString::new(s).unwrap()
    }
    fn revision(m: &std::fs::Metadata) -> (i64, i64, i64, i64) {
        (m.mtime(), m.mtime_nsec(), m.ctime(), m.ctime_nsec())
    }
    fn lock(file: &File) -> io::Result<()> {
        let m = file.metadata()?;
        if !m.is_file()
            || m.nlink() != 1
            || m.uid() != unsafe { libc::geteuid() }
            || m.mode() & 0o077 != 0
        {
            return Err(invalid("Journal no es archivo regular privado exclusivo"));
        }
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    impl Journal {
        /// Directory must already exist, private0700. No global config or mkdir.
        /// Creation/sync failure preserves any visible journal for manual review.
        pub fn create(path: &Path, manifest: Manifest) -> io::Result<Self> {
            Self::create_with(path, manifest, || Ok(()))
        }
        pub(super) fn create_with(
            path: &Path,
            manifest: Manifest,
            after_sync: impl FnOnce() -> io::Result<()>,
        ) -> io::Result<Self> {
            let wire = initial(&manifest)?;
            let directory = directory(path)?;
            let name = name(manifest.job());
            let mut file = owned(unsafe {
                libc::openat(
                    directory.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDWR
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_NOFOLLOW
                        | libc::O_NONBLOCK
                        | libc::O_CLOEXEC,
                    0o600,
                )
            })?;
            lock(&file)?;
            file.write_all(&wire)?;
            file.sync_all()?;
            directory.sync_all()?;
            after_sync()?;
            let state = State::new(&manifest).map_err(invalid)?;
            let length = wire.len() as u64;
            let revision = revision(&file.metadata()?);
            let mut journal = Self {
                owner_pid: unsafe { libc::getpid() },
                directory,
                file,
                name,
                manifest,
                state,
                sequence: 0,
                length,
                uncertain: true,
                revision,
            };
            journal.check_binding(length, Some(revision))?;
            journal.uncertain = false;
            Ok(journal)
        }
        pub fn open(path: &Path, job: [u8; 16]) -> io::Result<Self> {
            Self::open_directory(directory(path)?, job, MAX_JOURNAL)
        }
        pub fn open_single_file(path: &Path, job: [u8; 16]) -> io::Result<Self> {
            Self::open_directory(directory(path)?, job, MAX_SINGLE_FILE_JOURNAL)
        }
        /// Catalogue enumerates this exact FD; never reopens the root pathname.
        pub fn open_at_single_file(directory: &File, job: [u8; 16]) -> io::Result<Self> {
            Self::open_directory(directory.try_clone()?, job, MAX_SINGLE_FILE_JOURNAL)
        }
        fn open_directory(directory: File, job: [u8; 16], limit: usize) -> io::Result<Self> {
            let m = directory.metadata()?;
            if !m.is_dir() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o077 != 0 {
                return Err(invalid("Directorio journal no privado/propio"));
            }
            let name = name(job);
            let mut file = owned(unsafe {
                libc::openat(
                    directory.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDWR | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
                )
            })?;
            lock(&file)?;
            let before = file.metadata()?;
            if before.len() > limit as u64 {
                return Err(invalid("Journal excede límite"));
            }
            let mut bytes = vec![0u8; before.len() as usize];
            file.read_exact(&mut bytes)?;
            if file.read(&mut [0u8; 1])? != 0 {
                return Err(invalid("Journal creció durante lectura"));
            }
            let after = file.metadata()?;
            if after.len() != before.len()
                || after.mtime_nsec() != before.mtime_nsec()
                || after.mtime() != before.mtime()
                || after.ctime() != before.ctime()
                || after.ctime_nsec() != before.ctime_nsec()
            {
                return Err(invalid("Journal cambió durante lectura"));
            }
            let (manifest, state, sequence) = replay(&bytes)?;
            let revision = revision(&after);
            if manifest.job() != job {
                return Err(invalid("Identidad de job no coincide"));
            }
            let s = Self {
                owner_pid: unsafe { libc::getpid() },
                directory,
                file,
                name,
                manifest,
                state,
                sequence,
                length: bytes.len() as u64,
                uncertain: false,
                revision,
            };
            s.check_file()?;
            Ok(s)
        }
        pub fn manifest(&self) -> &Manifest {
            &self.manifest
        }
        pub fn state(&self) -> &State {
            &self.state
        }
        pub fn uncertain(&self) -> bool {
            self.uncertain
        }
        /// Proof of captured intent, including uncertain-ACK owners. Uses FD
        /// identities and owned manifest, not untrusted current filename bytes.
        /// Reopening must compare this proof BEFORE performing any mutation.
        pub fn intent_proof(&self) -> io::Result<IntentProof> {
            use sha2::{Digest, Sha256};
            fn id(file: &File, kind: Kind) -> io::Result<Identity> {
                let m = file.metadata()?;
                Ok(Identity {
                    device: m.dev(),
                    inode: m.ino(),
                    kind,
                })
            }
            let wire = self.manifest.encode().map_err(invalid)?;
            Ok(IntentProof {
                directory: id(&self.directory, Kind::Directory)?,
                journal: id(&self.file, Kind::File)?,
                manifest_sha256: Sha256::digest(&wire).into(),
            })
        }
        pub fn require_intent(&self, expected: &IntentProof) -> io::Result<()> {
            self.ensure_current()?;
            if self.intent_proof()? != *expected {
                return Err(invalid(
                    "Journal o intención sustituidos: revisar de nuevo antes de actuar",
                ));
            }
            Ok(())
        }
        pub fn ensure_current(&self) -> io::Result<()> {
            self.check_file()
        }
        #[cfg(test)]
        pub(crate) fn fail_after_record_sync(&mut self, event: Event) -> io::Result<()> {
            self.record_with(event, |point| {
                if point == 2 {
                    Err(io::Error::other("injected ACK failure"))
                } else {
                    Ok(())
                }
            })
        }
        fn check_file(&self) -> io::Result<()> {
            if self.uncertain {
                return Err(invalid(
                    "ACK journal incierto: detener mutaciones y reabrir",
                ));
            }
            self.check_binding(self.length, Some(self.revision))
                .map(|_| ())
        }
        fn check_binding(
            &self,
            expected_length: u64,
            expected_revision: Option<(i64, i64, i64, i64)>,
        ) -> io::Result<(i64, i64, i64, i64)> {
            let fd = self.file.metadata()?;
            let mut s = std::mem::MaybeUninit::<libc::stat>::uninit();
            if unsafe {
                libc::fstatat(
                    self.directory.as_raw_fd(),
                    self.name.as_ptr(),
                    s.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            } != 0
            {
                return Err(io::Error::last_os_error());
            }
            let s = unsafe { s.assume_init() };
            if s.st_dev != fd.dev()
                || s.st_ino != fd.ino()
                || s.st_mode & libc::S_IFMT != libc::S_IFREG
                || s.st_nlink != 1
                || s.st_uid != unsafe { libc::geteuid() }
                || s.st_mode & 0o077 != 0
                || fd.len() != expected_length
                || expected_revision.is_some_and(|expected| revision(&fd) != expected)
            {
                return Err(invalid("Journal reemplazado o modificado: no escribir"));
            }
            Ok(revision(&fd))
        }
        pub fn record(&mut self, e: Event) -> io::Result<()> {
            self.record_with(e, |_| Ok(()))
        }
        pub(super) fn record_with(
            &mut self,
            e: Event,
            mut fault: impl FnMut(u8) -> io::Result<()>,
        ) -> io::Result<()> {
            self.check_file()?;
            self.state
                .check(&self.manifest, e, false)
                .map_err(invalid)?;
            let next = self.sequence + 1;
            if next > (3 * self.manifest.entries().len() + 16) as u64 {
                return Err(invalid("Journal agotado"));
            }
            let event = encode_event(e);
            let mut record = Vec::with_capacity(116);
            record.extend_from_slice(&((8 + event.len()) as u32).to_le_bytes());
            record.extend_from_slice(&next.to_le_bytes());
            record.extend_from_slice(&event);
            let checksum = crc(&record[4..]);
            record.extend_from_slice(&checksum.to_le_bytes());
            if self.length + record.len() as u64 > MAX_JOURNAL as u64 {
                return Err(invalid("Journal agotado"));
            }
            // Once persistence starts, ANY failure is uncertain; even before a
            // failed ACK bytes may be visible. Confirmed State never advances.
            let was_fresh = self.state.fresh_barriers;
            self.uncertain = true;
            self.state.fresh_barriers = false;
            fault(0)?;
            self.file.seek(SeekFrom::End(0))?;
            self.file.write_all(&record)?;
            fault(1)?;
            self.file.sync_all()?;
            fault(2)?;
            let next_length = self.length + record.len() as u64;
            self.revision = self.check_binding(next_length, None)?;
            self.length = next_length;
            self.sequence = next;
            self.uncertain = false;
            // Previous fresh barrier remains applicable only within this live
            // instance. Caller rechecks each entry; reopening always clears it.
            self.state.fresh_barriers = was_fresh;
            self.state.commit(&self.manifest, e);
            Ok(())
        }
        /// Callback must reverify *current* destination contents/metadata and
        /// fsync destination+parents, even for a parsed DestinationSynced. This
        /// module additionally syncs journal+its parent; it does not inspect data.
        pub fn renew_barriers(
            &mut self,
            verify_and_sync: impl FnOnce(&Manifest, &State) -> io::Result<()>,
        ) -> io::Result<()> {
            self.renew_barriers_with(verify_and_sync, || Ok(()))
        }
        pub(super) fn renew_barriers_with(
            &mut self,
            verify_and_sync: impl FnOnce(&Manifest, &State) -> io::Result<()>,
            after_sync: impl FnOnce() -> io::Result<()>,
        ) -> io::Result<()> {
            self.check_file()?;
            self.state.fresh_barriers = false;
            self.state.checked_entry = None;
            if !matches!(self.state.phase, Phase::DestinationSynced | Phase::Deleting) {
                return Err(invalid("Destino no publicado/sincronizado"));
            }
            verify_and_sync(&self.manifest, &self.state)?;
            self.check_file()?;
            self.uncertain = true;
            self.file.sync_all()?;
            self.directory.sync_all()?;
            after_sync()?;
            self.check_binding(self.length, Some(self.revision))?;
            self.uncertain = false;
            self.state.fresh_barriers = true;
            Ok(())
        }
        /// Source restoration may abort an outstanding DeleteIntent only after
        /// full current source-presence/content verification by the executor.
        /// No source deletion is performed; reopening never inherits this proof.
        pub fn verify_restore(
            &mut self,
            verify: impl FnOnce(&Manifest, &State) -> io::Result<()>,
        ) -> io::Result<()> {
            self.state.restore_checked = false;
            self.check_file()?;
            if self.state.removed != 0
                || matches!(self.state.phase, Phase::Completed | Phase::Restored)
            {
                return Err(invalid(
                    "No se puede restaurar íntegro un origen ya eliminado",
                ));
            }
            verify(&self.manifest, &self.state)?;
            self.check_file()?;
            self.state.restore_checked = true;
            Ok(())
        }
        /// Fresh per-entry observations; still requires second identity check in
        /// executor immediately before syscall. No deletion is performed here.
        pub fn verify_entry(
            &mut self,
            index: u32,
            verify: impl FnOnce(&Manifest, &State, u32) -> io::Result<()>,
        ) -> io::Result<()> {
            self.check_file()?;
            self.state.checked_entry = None;
            if !self.state.fresh_barriers
                || self.state.deletion_index(&self.manifest) != Some(index)
            {
                return Err(invalid("Falta barrera actual o entrada fuera de orden"));
            }
            verify(&self.manifest, &self.state, index)?;
            self.check_file()?;
            self.state.checked_entry = Some(index);
            Ok(())
        }
    }
    #[cfg(test)]
    mod owner_drop_tests {
        use super::*;
        #[test]
        fn owner_release_does_not_wait_for_an_inherited_open_description_to_close() {
            use std::os::unix::fs::PermissionsExt;
            let root = crate::persistence::test_dir("journal-owner-release");
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
            let owner = Journal::create(&root, crate::move_manifest::tests::manifest()).unwrap();
            let inherited = owner.file.try_clone().unwrap();
            assert!(Journal::open(&root, [1; 16]).is_err());
            drop(owner);
            let fresh = Journal::open(&root, [1; 16])
                .expect("owner release must unlock even with a duplicate description alive");
            drop(inherited);
            assert!(Journal::open(&root, [1; 16]).is_err());
            drop(fresh);
            assert!(Journal::open(&root, [1; 16]).is_ok());
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}
#[cfg(target_os = "linux")]
pub use linux::Journal;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::move_manifest::tests::{entry, manifest};
    pub(super) fn source_staged(m: &Manifest) -> Event {
        Event::SourceStaged {
            root_revision: m.entries()[0].revision,
        }
    }
    pub(super) fn destination_published(m: &Manifest) -> Event {
        Event::DestinationPublished {
            root_revision: m.entries()[0].revision,
        }
    }
    pub(super) fn staged(m: &Manifest) -> Event {
        Event::StageIntent {
            source_stage: Identity {
                device: m.source_identity().device,
                inode: 100,
                kind: Kind::Directory,
            },
            destination_stage: Identity {
                device: m.destination_identity().device,
                inode: 200,
                kind: Kind::Directory,
            },
        }
    }
    fn verified(m: &Manifest, i: usize) -> Event {
        let e = &m.entries()[i];
        Event::EntryVerified {
            index: i as u32,
            destination: Identity {
                device: m.destination_identity().device,
                inode: e.identity.inode + 1000,
                kind: e.identity.kind,
            },
            content: e.content_digest,
            metadata: e.metadata_digest,
        }
    }
    pub(super) fn prefix(m: &Manifest) -> Vec<Event> {
        let mut events = vec![staged(m), source_staged(m)];
        for i in 0..m.entries().len() {
            events.push(verified(m, i));
        }
        events.extend([
            Event::PublishIntent,
            destination_published(m),
            Event::DestinationSynced,
        ]);
        events
    }
    fn append(out: &mut Vec<u8>, sequence: u64, event: Event) {
        let mut payload = sequence.to_le_bytes().to_vec();
        payload.extend(encode_event(event));
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&payload);
        out.extend_from_slice(&crc(&payload).to_le_bytes());
    }
    #[test]
    fn write_ahead_order_children_first_and_recovery_never_inherits_barriers() {
        let m = manifest();
        let mut state = State::new(&m).unwrap();
        assert!(state.check(&m, Event::DestinationSynced, false).is_err());
        let mut bytes = initial(&m).unwrap();
        for (i, e) in prefix(&m).into_iter().enumerate() {
            state.check(&m, e, false).unwrap();
            state.commit(&m, e);
            append(&mut bytes, i as u64 + 1, e);
        }
        assert_eq!(state.phase(), Phase::DestinationSynced);
        assert!(state.check(&m, Event::DeleteIntent(1), false).is_err());
        state.fresh_barriers = true;
        state.checked_entry = Some(1);
        assert!(state.check(&m, Event::DeleteIntent(0), false).is_err());
        state.check(&m, Event::DeleteIntent(1), false).unwrap();
        state.commit(&m, Event::DeleteIntent(1));
        state.check(&m, Event::Removed(1), false).unwrap();
        state.commit(&m, Event::Removed(1));
        assert!(state.check(&m, Event::DeleteIntent(0), false).is_err()); // per-entry check renewed
        state.checked_entry = Some(0);
        state.check(&m, Event::DeleteIntent(0), false).unwrap();
        state.commit(&m, Event::DeleteIntent(0));
        state.check(&m, Event::Removed(0), false).unwrap();
        state.commit(&m, Event::Removed(0));
        state.check(&m, Event::Completed, false).unwrap();
        let (_, reopened, _) = replay(&bytes).unwrap();
        assert_eq!(reopened.phase(), Phase::DestinationSynced);
        assert!(!reopened.barriers_current());
        assert!(reopened.check(&m, Event::DeleteIntent(1), false).is_err());
    }
    #[test]
    fn destination_proofs_preserve_hardlinks_and_cannot_alias_unrelated_sources() {
        let mut a = entry(b"folder/a", Kind::File);
        a.revision.links = 2;
        let mut b = entry(b"folder/b", Kind::File);
        b.revision.links = 2;
        let m = Manifest::new(
            [2; 16],
            b"/a".to_vec(),
            b"/b".to_vec(),
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
            vec![entry(b"folder", Kind::Directory), a, b],
        )
        .unwrap();
        let mut state = State::new(&m).unwrap();
        state.commit(&m, staged(&m));
        state.commit(&m, source_staged(&m));
        let directory = verified(&m, 0);
        state.check(&m, directory, false).unwrap();
        state.commit(&m, directory);
        let proof = verified(&m, 1);
        state.check(&m, proof, false).unwrap();
        state.commit(&m, proof);
        let mut bad = verified(&m, 2);
        if let Event::EntryVerified { destination, .. } = &mut bad {
            destination.inode += 1;
        }
        assert!(state.check(&m, bad, false).is_err());
        state.check(&m, verified(&m, 2), false).unwrap();
        let m = manifest();
        let mut state = State::new(&m).unwrap();
        state.commit(&m, staged(&m));
        state.commit(&m, source_staged(&m));
        let mut bad = verified(&m, 0);
        if let Event::EntryVerified { content, .. } = &mut bad {
            content[0] ^= 1;
        }
        assert!(state.check(&m, bad, false).is_err());
        let first = verified(&m, 0);
        state.commit(&m, first);
        let mut bad = verified(&m, 1);
        if let Event::EntryVerified { destination, .. } = &mut bad {
            destination.inode = state.destinations[0].inode;
        }
        assert!(state.check(&m, bad, false).is_err());
    }
    #[test]
    fn torn_corrupt_future_and_reordered_records_fail_without_skipping() {
        let m = manifest();
        let mut bytes = initial(&m).unwrap();
        let header = bytes.len();
        let mut boundaries = vec![header];
        for (i, e) in prefix(&m).into_iter().enumerate() {
            append(&mut bytes, i as u64 + 1, e);
            boundaries.push(bytes.len());
        }
        for n in 0..bytes.len() {
            assert_eq!(
                replay(&bytes[..n]).is_ok(),
                boundaries.contains(&n),
                "prefix {n}"
            );
        }
        let mut bad = bytes.clone();
        bad[7] = 99;
        assert!(replay(&bad).is_err());
        let mut bad = bytes.clone();
        let last = bad.len() - 1;
        bad[last] ^= 1;
        assert!(replay(&bad).is_err());
        let mut bad = initial(&m).unwrap();
        append(&mut bad, 2, staged(&m));
        assert!(replay(&bad).is_err());
        let mut bad = initial(&m).unwrap();
        append(&mut bad, 1, source_staged(&m));
        assert!(replay(&bad).is_err());
    }
    #[test]
    fn restored_is_not_completed_and_partial_delete_cannot_claim_full_restore() {
        let m = manifest();
        let mut state = State::new(&m).unwrap();
        assert!(state.check(&m, Event::RestoreIntent, false).is_err());
        state.restore_checked = true;
        state.check(&m, Event::RestoreIntent, false).unwrap();
        state.commit(&m, Event::RestoreIntent);
        state.check(&m, Event::Restored, false).unwrap();
        state.commit(&m, Event::Restored);
        assert_ne!(state.phase(), Phase::Completed);
        assert!(state.check(&m, Event::PublishIntent, false).is_err());
        let mut state = State::new(&m).unwrap();
        for e in prefix(&m) {
            state.commit(&m, e);
        }
        state.fresh_barriers = true;
        state.checked_entry = Some(1);
        state.commit(&m, Event::DeleteIntent(1));
        assert!(state.check(&m, Event::RestoreIntent, false).is_err());
        state.commit(&m, Event::Removed(1));
        assert!(state.check(&m, Event::RestoreIntent, false).is_err());
    }
    #[test]
    fn crc_known_vector() {
        assert_eq!(crc(b"123456789"), 0xcbf43926);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod store_tests {
    use super::*;
    use std::{
        ffi::CString,
        fs,
        os::unix::{
            ffi::OsStrExt,
            fs::{PermissionsExt, symlink},
        },
    };
    fn directory(label: &str) -> std::path::PathBuf {
        let d = crate::persistence::test_dir(label);
        fs::set_permissions(&d, fs::Permissions::from_mode(0o700)).unwrap();
        d
    }
    fn path(d: &std::path::Path) -> std::path::PathBuf {
        d.join("move-01010101010101010101010101010101.journal")
    }
    fn ready(j: &mut Journal) {
        let events = super::tests::prefix(j.manifest());
        for e in events {
            j.record(e).unwrap();
        }
    }
    #[test]
    fn lock_regular_nofollow_fifo_future_and_exclusive_create() {
        let d = directory("move-journal-open");
        let j = Journal::create(&d, crate::move_manifest::tests::manifest()).unwrap();
        assert!(Journal::open(&d, [1; 16]).is_err());
        assert!(Journal::create(&d, crate::move_manifest::tests::manifest()).is_err());
        drop(j);
        let original = fs::read(path(&d)).unwrap();
        let mut future = original.clone();
        future[7] = 99;
        fs::write(path(&d), &future).unwrap();
        assert!(Journal::open(&d, [1; 16]).is_err());
        assert_eq!(fs::read(path(&d)).unwrap(), future);
        fs::remove_file(path(&d)).unwrap();
        let target = d.join("foreign");
        fs::write(&target, b"untouched").unwrap();
        symlink(&target, path(&d)).unwrap();
        assert!(Journal::open(&d, [1; 16]).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"untouched");
        fs::remove_file(path(&d)).unwrap();
        let c = CString::new(path(&d).as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        assert!(Journal::open(&d, [1; 16]).is_err());
        fs::remove_file(path(&d)).unwrap();
        fs::write(path(&d), &original).unwrap();
        let file = fs::OpenOptions::new().write(true).open(path(&d)).unwrap();
        file.set_len(MAX_JOURNAL as u64 + 1).unwrap();
        assert!(Journal::open(&d, [1; 16]).is_err());
        assert_eq!(
            fs::metadata(path(&d)).unwrap().len(),
            MAX_JOURNAL as u64 + 1
        );
        fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn replacement_and_live_same_length_edit_are_not_overwritten() {
        let d = directory("move-journal-replaced");
        let mut j = Journal::create(&d, crate::move_manifest::tests::manifest()).unwrap();
        fs::rename(path(&d), d.join("preserved")).unwrap();
        fs::write(path(&d), b"foreign").unwrap();
        assert!(j.record(super::tests::staged(j.manifest())).is_err());
        assert_eq!(fs::read(path(&d)).unwrap(), b"foreign");
        drop(j);
        fs::remove_file(path(&d)).unwrap();
        fs::rename(d.join("preserved"), path(&d)).unwrap();
        let mut j = Journal::open(&d, [1; 16]).unwrap();
        let mut bytes = fs::read(path(&d)).unwrap();
        bytes[0] ^= 1;
        fs::write(path(&d), &bytes).unwrap();
        assert!(j.record(super::tests::staged(j.manifest())).is_err());
        assert_eq!(fs::read(path(&d)).unwrap(), bytes);
        fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn readable_synced_event_after_failed_ack_requires_new_barriers_and_entry_checks() {
        let d = directory("move-journal-ack");
        let mut j = Journal::create(&d, crate::move_manifest::tests::manifest()).unwrap();
        let mut events = super::tests::prefix(j.manifest());
        assert_eq!(events.pop(), Some(Event::DestinationSynced));
        for e in events {
            j.record(e).unwrap();
        }
        let result = j.record_with(Event::DestinationSynced, |step| {
            if step == 2 {
                Err(io::Error::other("ACK failed after fsync"))
            } else {
                Ok(())
            }
        });
        assert!(result.is_err());
        assert!(j.uncertain());
        assert_eq!(j.state().phase(), Phase::DestinationPublished);
        assert!(j.record(Event::DestinationSynced).is_err());
        drop(j);
        let mut j = Journal::open(&d, [1; 16]).unwrap();
        assert_eq!(j.state().phase(), Phase::DestinationSynced);
        assert!(!j.state().barriers_current());
        assert!(j.verify_entry(1, |_, _, _| Ok(())).is_err());
        assert!(j.record(Event::DeleteIntent(1)).is_err());
        assert!(
            j.renew_barriers(|_, _| Err(io::Error::other("destination sync failed")))
                .is_err()
        );
        assert!(!j.state().barriers_current());
        j.renew_barriers(|_, _| Ok(())).unwrap();
        assert!(
            j.verify_entry(1, |_, _, _| Err(io::Error::other("source changed")))
                .is_err()
        );
        assert!(j.record(Event::DeleteIntent(1)).is_err());
        j.verify_entry(1, |_, _, _| Ok(())).unwrap();
        j.record(Event::DeleteIntent(1)).unwrap();
        drop(j);
        let mut j = Journal::open(&d, [1; 16]).unwrap();
        assert_eq!(j.state().pending_delete(), Some(1));
        assert!(j.record(Event::Removed(1)).is_err());
        j.renew_barriers(|_, _| Ok(())).unwrap();
        j.verify_entry(1, |_, _, _| Ok(())).unwrap();
        j.record(Event::Removed(1)).unwrap();
        assert!(j.record(Event::DeleteIntent(0)).is_err());
        j.verify_entry(0, |_, _, _| Ok(())).unwrap();
        j.record(Event::DeleteIntent(0)).unwrap();
        j.record(Event::Removed(0)).unwrap();
        j.record(Event::Completed).unwrap();
        drop(j);
        let j = Journal::open(&d, [1; 16]).unwrap();
        assert_eq!(j.state().phase(), Phase::Completed);
        assert!(!j.state().barriers_current());
        drop(j);
        fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn every_persistence_failure_stops_live_mutations_and_preserves_visible_bytes() {
        for point in 0..=2 {
            let d = directory("move-journal-fault");
            let mut j = Journal::create(&d, crate::move_manifest::tests::manifest()).unwrap();
            let event = super::tests::staged(j.manifest());
            assert!(
                j.record_with(event, |step| if step == point {
                    Err(io::Error::other("injected persistence failure"))
                } else {
                    Ok(())
                })
                .is_err()
            );
            assert!(j.uncertain());
            assert_eq!(j.state().phase(), Phase::Prepared);
            assert!(j.record(event).is_err());
            let bytes = fs::read(path(&d)).unwrap();
            drop(j);
            let recovered = Journal::open(&d, [1; 16]).unwrap();
            assert_eq!(
                recovered.state().phase(),
                if point == 0 {
                    Phase::Prepared
                } else {
                    Phase::StageIntent
                }
            );
            assert_eq!(fs::read(path(&d)).unwrap(), bytes);
            drop(recovered);
            fs::remove_dir_all(d).unwrap();
        }
    }
    #[test]
    fn reopen_replays_but_never_truncates_partial_tail() {
        let d = directory("move-journal-torn");
        let mut j = Journal::create(&d, crate::move_manifest::tests::manifest()).unwrap();
        ready(&mut j);
        drop(j);
        let mut bytes = fs::read(path(&d)).unwrap();
        bytes.pop();
        fs::write(path(&d), &bytes).unwrap();
        assert!(Journal::open(&d, [1; 16]).is_err());
        assert_eq!(fs::read(path(&d)).unwrap(), bytes);
        fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn replacement_during_write_sync_create_or_renew_never_receives_ack() {
        for step in [1, 2] {
            let d = directory("move-journal-postcheck");
            let mut j = Journal::create(&d, crate::move_manifest::tests::manifest()).unwrap();
            let event = super::tests::staged(j.manifest());
            let result = j.record_with(event, |point| {
                if point == step {
                    fs::rename(path(&d), d.join("preserved"))?;
                    fs::write(path(&d), b"foreign")?;
                }
                Ok(())
            });
            assert!(result.is_err());
            assert!(j.uncertain());
            assert_eq!(j.state().phase(), Phase::Prepared);
            assert_eq!(fs::read(path(&d)).unwrap(), b"foreign");
            drop(j);
            fs::remove_dir_all(d).unwrap();
        }
        let d = directory("move-journal-create-postcheck");
        assert!(
            Journal::create_with(&d, crate::move_manifest::tests::manifest(), || {
                fs::rename(path(&d), d.join("preserved"))?;
                fs::write(path(&d), b"foreign")
            })
            .is_err()
        );
        assert_eq!(fs::read(path(&d)).unwrap(), b"foreign");
        fs::remove_dir_all(d).unwrap();
        let d = directory("move-journal-renew-postcheck");
        let mut j = Journal::create(&d, crate::move_manifest::tests::manifest()).unwrap();
        ready(&mut j);
        assert!(
            j.renew_barriers_with(
                |_, _| Ok(()),
                || {
                    fs::rename(path(&d), d.join("preserved"))?;
                    fs::write(path(&d), b"foreign")
                }
            )
            .is_err()
        );
        assert!(j.uncertain());
        assert!(!j.state().barriers_current());
        assert_eq!(fs::read(path(&d)).unwrap(), b"foreign");
        drop(j);
        fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn multiroot_valid_crc_wire_cannot_create_open_or_construct_state() {
        use crate::move_manifest::tests::entry;
        let mut a = entry(b"a", Kind::File);
        a.revision.links = 2;
        let mut b = entry(b"b", Kind::File);
        b.revision.links = 2;
        let m = Manifest::new(
            [1; 16],
            b"/a".to_vec(),
            b"/b".to_vec(),
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
            vec![a, b],
        )
        .unwrap();
        assert_eq!(m.root_count(), 2);
        assert!(State::new(&m).is_err());
        let wire = m.encode().unwrap();
        let d = directory("move-journal-multiroot");
        assert!(Journal::create(&d, m).is_err());
        assert!(!path(&d).exists());
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&(wire.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&wire);
        let checksum = crc(&bytes);
        bytes.extend_from_slice(&checksum.to_le_bytes());
        fs::write(path(&d), &bytes).unwrap();
        fs::set_permissions(path(&d), fs::Permissions::from_mode(0o600)).unwrap();
        assert!(Journal::open(&d, [1; 16]).is_err());
        assert_eq!(fs::read(path(&d)).unwrap(), bytes);
        assert!(replay(&bytes).is_err());
        fs::remove_dir_all(d).unwrap();
    }
}
