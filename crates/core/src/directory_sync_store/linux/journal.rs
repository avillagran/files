//! Durable declarations and explicit trusted-coordinator namespace execution.
//! Reopening/renewal never establishes native approval or source freshness.
use super::*;
use crate::directory_sync_journal::{self as wire, Event, Schema};

pub struct Journal<'a> {
    store: &'a Store,
    placements: Option<PlacementRegistry<'a>>,
    directory: Directory,
    held: Held,
    name: CString,
    identity: Node,
    stamp: Stamp,
    schema: Schema,
    replay: wire::Replay,
    state: State,
}
impl<'a> Journal<'a> {
    pub fn create(
        store: &'a Store,
        physical: &Physical<'_, '_>,
        cancel: &AtomicBool,
    ) -> io::Result<Self> {
        check_cancel(cancel)?;
        store.ensure_current()?;
        let mut journal = Self::acquire(store, physical, true)?;
        journal
            .held
            .file
            .write_all(&wire::header(&journal.schema))?;
        // Capture our write revision BEFORE readback; do not absorb a later
        // unrelated rewrite into the writer's own timestamp allowance.
        journal.stamp = check_binding(
            &journal.directory,
            &journal.held,
            &journal.name,
            journal.identity,
            None,
        )?;
        journal.held.file.seek(SeekFrom::Start(0))?;
        journal.replay = wire::replay(&journal.schema, &mut journal.held.file)?;
        if journal.replay.length != wire::HEADER as u64 {
            return Err(bad("Journal inicial modificado"));
        }
        journal.binding()?;
        journal.confirm(cancel, &mut |_| Ok(()))?;
        Ok(journal)
    }
    pub fn open(store: &'a Store, physical: &Physical<'_, '_>) -> io::Result<Self> {
        let mut journal = Self::acquire(store, physical, false)?;
        journal.replay = wire::replay(&journal.schema, &mut journal.held.file)?;
        journal.binding()?;
        Ok(journal)
    }
    fn acquire(store: &'a Store, physical: &Physical<'_, '_>, create: bool) -> io::Result<Self> {
        let mut schema = Schema::derive(store, physical)?;
        if create {
            schema.staging_admission()?;
        }
        let directory = Directory::open(&store.directory.path)?;
        directory.check()?;
        let job = &store.proof.encode()[56..72];
        let mut text = String::from("sync-effects-");
        for b in job {
            use std::fmt::Write;
            write!(&mut text, "{b:02x}").expect("string");
        }
        text.push_str(".journal");
        let name = c(text.as_bytes())?;
        let file = if create {
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
            if unsafe { libc::fchmod(file.as_raw_fd(), 0o600) } != 0 {
                return Err(io::Error::last_os_error());
            }
            file
        } else {
            let pin = owned(unsafe {
                libc::openat(
                    directory.file.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            })?;
            private_file(&pin)?;
            let path = c(format!("/proc/self/fd/{}", pin.as_raw_fd()).as_bytes())?;
            let file = owned(unsafe {
                libc::open(
                    path.as_ptr(),
                    libc::O_RDWR | libc::O_NOATIME | libc::O_NONBLOCK | libc::O_CLOEXEC,
                )
            })?;
            if node(&file)? != node(&pin)? {
                return Err(bad("Journal sustituido"));
            }
            file
        };
        let held = Held::lock(file)?;
        let identity = node(&held.file)?;
        let stamp = check_binding(&directory, &held, &name, identity, None)?;
        if stamp.length > wire::MAX_JOURNAL {
            return Err(bad("Journal excesivo"));
        }
        schema.protect(identity.device, identity.inode);
        let replay = wire::replay(&schema, &mut wire::header(&schema).as_slice())?;
        Ok(Self {
            placements: None,
            store,
            directory,
            held,
            name,
            identity,
            stamp,
            schema,
            replay,
            state: State::History,
        })
    }
    fn binding(&self) -> io::Result<()> {
        self.store
            .require_proof(&StoredProof::decode(&self.schema.proof().encode())?)?;
        check_binding(
            &self.directory,
            &self.held,
            &self.name,
            self.identity,
            Some(self.stamp),
        )?;
        Ok(())
    }
    /// Identity comparison data. This tuple is never execution authority.
    pub fn identity(&self) -> (u64, u64, u64) {
        (
            self.identity.device,
            self.identity.inode,
            self.identity.mount,
        )
    }
    /// A retained identity can reject a byte-identical replacement on reopen.
    /// This compares provenance data only; it does not renew durability.
    pub fn require_identity(&self, expected: (u64, u64, u64)) -> io::Result<()> {
        self.binding()?;
        if self.identity() != expected {
            return Err(bad("Identidad original de journal no coincide"));
        }
        Ok(())
    }
    pub fn path(&self) -> PathBuf {
        self.directory
            .path
            .join(OsString::from_vec(self.name.as_bytes().to_vec()))
    }
    pub fn durability(&self) -> State {
        self.state
    }
    pub fn history(&self) -> &wire::Replay {
        &self.replay
    }
    pub fn ensure_current(&self) -> io::Result<()> {
        if self.state != State::Durable {
            return Err(bad("Journal sin ACK durable vigente"));
        }
        self.store.ensure_current()?;
        self.binding()?;
        if let Some(seal) = self.replay.state.placement_seal() {
            self.placements
                .as_ref()
                .ok_or_else(|| bad("Registro de placements vivo requerido"))?
                .matches(self.schema.proof(), seal)?;
        }
        Ok(())
    }
    /// Artifact renewal/binding only; never native approval or source freshness.
    pub fn attach_placements(&mut self, registry: PlacementRegistry<'a>) -> io::Result<()> {
        if self.state != State::Durable {
            return Err(bad("Journal no renovado"));
        }
        self.store.ensure_current()?;
        self.binding()?;
        let seal = self
            .replay
            .state
            .placement_seal()
            .ok_or_else(|| bad("Journal sin registro de placements"))?;
        registry.matches(self.schema.proof(), seal)?;
        self.placements = Some(registry);
        Ok(())
    }
    pub fn placements(&self) -> Option<&PlacementRegistry<'a>> {
        self.placements.as_ref()
    }
    pub fn renew(&mut self, cancel: &AtomicBool) -> io::Result<()> {
        if self.state == State::Uncertain {
            return Err(bad("ACK incierto: requiere reapertura"));
        }
        self.store.ensure_current()?;
        self.binding()?;
        self.state = State::Uncertain;
        self.held.file.seek(SeekFrom::Start(0))?;
        let replay = wire::replay(&self.schema, &mut self.held.file)?;
        if replay.length != self.stamp.length {
            return Err(bad("Longitud de journal cambió"));
        }
        self.replay = replay;
        self.confirm(cancel, &mut |_| Ok(()))
    }
    pub fn append(&mut self, event: Event, cancel: &AtomicBool) -> io::Result<()> {
        self.append_with(event, cancel, |_| Ok(()))
    }
    pub(super) fn append_with(
        &mut self,
        event: Event,
        cancel: &AtomicBool,
        mut hook: impl FnMut(Barrier) -> io::Result<()>,
    ) -> io::Result<()> {
        check_cancel(cancel)?;
        self.ensure_current()?;
        // Reader compatibility: old tag32 streams remain historical data.
        // Only newly written intents require the registered live artifact.
        if matches!(event, Event::WorkspaceCreateIntent { .. })
            && self.replay.state.placement_seal().is_none()
        {
            return Err(bad(
                "Nueva intención de workspace requiere registro de placements",
            ));
        }
        self.replay.state.check(&self.schema, event)?;
        let bytes = wire::frame(self.replay.sequence + 1, self.replay.tail, event)?;
        let length = self
            .replay
            .length
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| bad("Journal excesivo"))?;
        if length > wire::MAX_JOURNAL {
            return Err(bad("Journal excesivo"));
        }
        self.state = State::Uncertain;
        self.held.file.seek(SeekFrom::Start(self.replay.length))?;
        self.held.file.write_all(&bytes)?;
        hook(Barrier::Write)?;
        self.stamp = check_binding(&self.directory, &self.held, &self.name, self.identity, None)?;
        if self.stamp.length != length {
            return Err(bad("Longitud de journal cambió"));
        }
        // Our own write masks timestamp changes by concurrent writers. Read
        // the whole chain against our expected tail, not just the new frame.
        self.held.file.seek(SeekFrom::Start(0))?;
        let replay = wire::replay(&self.schema, &mut self.held.file)?;
        if replay.length != length
            || replay.sequence != self.replay.sequence + 1
            || replay.tail != bytes[bytes.len() - 32..]
        {
            return Err(bad("Readback de journal distinto de escritura"));
        }
        self.binding()?;
        hook(Barrier::ReadBack)?;
        self.confirm(cancel, &mut hook)?;
        self.replay = replay;
        Ok(())
    }
    fn confirm(
        &mut self,
        cancel: &AtomicBool,
        hook: &mut impl FnMut(Barrier) -> io::Result<()>,
    ) -> io::Result<()> {
        self.state = State::Uncertain;
        check_cancel(cancel)?;
        self.binding()?;
        self.held.file.sync_all()?;
        hook(Barrier::FileSynced)?;
        self.directory.sync_namespace(hook)?;
        hook(Barrier::Acknowledged)?;
        check_cancel(cancel)?;
        self.store.ensure_current()?;
        self.binding()?;
        self.state = State::Durable;
        Ok(())
    }
}

mod own;
pub use own::OwnNamespaceRename;

mod staging;
pub use staging::{StagingReceipt, StagingWorkspace};

#[cfg(test)]
pub(super) use staging::StagingPoint;

mod create;
pub use create::{CreatedStageNode, CreationEvidence, StageCreateFailure};

#[cfg(test)]
pub(super) use create::Point as CreatePoint;

mod metadata;
#[cfg(test)]
pub(super) use metadata::MetadataPoint;
pub use metadata::{JournalMetadataFailure, OwnStageMetadata};

mod workspace;
#[cfg(test)]
pub(super) use workspace::WorkspacePoint;
pub use workspace::{CreatedWorkspace, WorkspaceFailure, WorkspaceParent};

mod batch;
pub use batch::{
    StageBatchFailure, StageBatchPhase, StageBatchPoint, StageBatchReport, StagedBatch,
    WorkspaceLocation,
};
