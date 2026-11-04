use super::*;
use crate::{
    directory_sync_journal::{Object, Observation},
    move_manifest::{put_identity, put_revision},
};
#[derive(Debug)]
pub struct NamespaceFailure {
    pub cause: io::Error,
    pub intent_recorded: bool,
    pub syscall_attempted: bool,
    pub syscall_succeeded: bool,
}
impl std::fmt::Display for NamespaceFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} (intent={}, attempted={}, returned_success={})",
            self.cause, self.intent_recorded, self.syscall_attempted, self.syscall_succeeded
        )
    }
}
impl std::error::Error for NamespaceFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnPoint {
    IntentWritten,
    IntentFileSynced,
    IntentAcknowledged,
    IntentDurable,
    BeforeSyscall,
    Dispatch,
    RenameReturned,
    Verified,
    DataSynced,
    Confirmed,
}
#[derive(Clone, Copy)]
pub(crate) struct NamespaceControl<'a> {
    pub cancel: &'a AtomicBool,
    pub cooperative: Option<&'a Control>,
}
pub(crate) trait NamespaceLog {
    fn intent(&mut self, seal: [u8; 32]) -> io::Result<()>;
    fn ready(&mut self, seal: [u8; 32]) -> io::Result<()>;
    fn confirmed(&mut self, change: &RenameChange) -> io::Result<()>;
    fn checkpoint(&mut self, _point: OwnPoint) -> io::Result<()> {
        Ok(())
    }
}
pub(crate) struct NamespaceBinding {
    pub from_root: Observation,
    pub to_root: Observation,
    pub from_parent: Object,
    pub to_parent: Object,
    pub from: Vec<u8>,
    pub to: Vec<u8>,
    pub source: Observation,
}
pub(super) struct Endpoints {
    from: usize,
    to: usize,
    source_index: usize,
    from_parent: usize,
    to_parent: usize,
    old: Vec<u8>,
    new: Vec<u8>,
    from_relative: Vec<u8>,
    to_relative: Vec<u8>,
    seal: [u8; 32],
}
fn object(e: &Entry, mount: u64) -> Object {
    Object {
        identity: e.identity,
        mount,
    }
}
fn observation(e: &Entry, mount: u64) -> Observation {
    Observation {
        object: object(e, mount),
        revision: e.revision,
        content: e.content_digest,
        metadata: e.metadata_digest,
    }
}
impl Endpoints {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        scopes: &[NamespaceScope<'_>],
        from: NamespacePath<'_>,
        to: NamespacePath<'_>,
        source_index: usize,
        from_parent: usize,
        to_parent: usize,
        old: &[u8],
        new: &[u8],
    ) -> Self {
        let mut h = Sha256::new();
        h.update(b"Files:namespace-own-rename:v1\0");
        h.update((scopes.len() as u32).to_le_bytes());
        h.update((from.scope as u32).to_le_bytes());
        h.update((to.scope as u32).to_le_bytes());
        sized(&mut h, from.relative);
        sized(&mut h, to.relative);
        for scope in scopes {
            h.update((scope.inventory.entries.len() as u32).to_le_bytes());
            sized(&mut h, scope.name.as_bytes());
            for (e, m) in scope
                .inventory
                .entries
                .iter()
                .zip(scope.inventory.mounts.as_ref().expect("sync"))
            {
                sized(&mut h, &e.relative);
                let mut b = Vec::new();
                put_identity(&mut b, e.identity);
                put_revision(&mut b, e.revision);
                h.update(b);
                h.update(e.access.seconds.to_le_bytes());
                h.update(e.access.nanos.to_le_bytes());
                h.update(e.content_digest);
                h.update(e.metadata_digest);
                h.update(m.to_le_bytes());
            }
        }
        Self {
            from: from.scope,
            to: to.scope,
            source_index,
            from_parent,
            to_parent,
            old: old.to_vec(),
            new: new.to_vec(),
            from_relative: from.relative.to_vec(),
            to_relative: to.relative.to_vec(),
            seal: h.finalize().into(),
        }
    }
}
impl NamespaceMove<'_> {
    pub(crate) fn intent_seal(&self) -> [u8; 32] {
        self.endpoints.seal
    }
    pub(crate) fn binding(&self) -> NamespaceBinding {
        let e = &self.endpoints;
        let a = &self.scopes[e.from].inventory;
        let b = &self.scopes[e.to].inventory;
        let am = a.mounts.as_ref().expect("sync");
        let bm = b.mounts.as_ref().expect("sync");
        NamespaceBinding {
            from_root: observation(&a.entries[0], am[0]),
            to_root: observation(&b.entries[0], bm[0]),
            from_parent: object(&a.entries[e.from_parent], am[e.from_parent]),
            to_parent: object(&b.entries[e.to_parent], bm[e.to_parent]),
            from: e.from_relative.clone(),
            to: e.to_relative.clone(),
            source: observation(&a.entries[e.source_index], am[e.source_index]),
        }
    }
    pub(crate) fn source_items(&self) -> impl Iterator<Item = (&[u8], Observation)> {
        let e = &self.endpoints;
        let scope = &self.scopes[e.from];
        scope
            .inventory
            .entries
            .iter()
            .zip(scope.inventory.mounts.as_ref().expect("sync"))
            .filter_map(move |(entry, m)| {
                if !under(&entry.relative, &e.old) {
                    return None;
                }
                let suffix = &entry.relative[e.old.len()..];
                Some((
                    suffix.strip_prefix(b"/").unwrap_or(suffix),
                    observation(entry, *m),
                ))
            })
    }
    fn sync_after(&self, cancel: &AtomicBool, control: Option<&Control>) -> io::Result<()> {
        let target = self.endpoints.to;
        let scope = &self.scopes[target];
        let c = &self.candidates[target];
        for (index, e) in c
            .entries
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, e)| under(&e.relative, &self.endpoints.new))
        {
            check_control(cancel, control)?;
            let (parent, pin, name) = super::super::sync_metadata::pin_entries(
                scope.parent,
                scope.name,
                &c.entries,
                &c.mounts,
                index,
                cancel,
            )?;
            if e.identity.kind != Kind::Symlink {
                let fd = data(
                    &parent,
                    &name,
                    e.identity,
                    e.identity.kind == Kind::Directory,
                )?;
                fd.sync_all()?;
                stable(&parent, &name, &fd, e.identity, e.revision)?;
            }
            // Symlink inode FDs are O_PATH; synchronize its directory, never follow.
            let parent_path = parent_path(&e.relative);
            let pi = c
                .entries
                .binary_search_by(|p| p.relative.as_slice().cmp(parent_path))
                .map_err(|_| invalid("Padre sync ausente"))?;
            let (gp, _, pn) = super::super::sync_metadata::pin_entries(
                scope.parent,
                scope.name,
                &c.entries,
                &c.mounts,
                pi,
                cancel,
            )?;
            let directory = data(&gp, &pn, c.entries[pi].identity, true)?;
            directory.sync_all()?;
            stable(&parent, &name, &pin, e.identity, e.revision)?;
        }
        for p in &self.parents {
            let s = &self.scopes[p.scope];
            let c = &self.candidates[p.scope];
            let (gp, _, name) = super::super::sync_metadata::pin_entries(
                s.parent, s.name, &c.entries, &c.mounts, p.index, cancel,
            )?;
            let fd = data(&gp, &name, c.entries[p.index].identity, true)?;
            fd.sync_all()?;
            stable(
                &gp,
                &name,
                &fd,
                c.entries[p.index].identity,
                c.entries[p.index].revision,
            )?;
        }
        Ok(())
    }
    pub(crate) fn execute_own(
        self,
        read: u64,
        controls: NamespaceControl<'_>,
        log: &mut impl NamespaceLog,
    ) -> Result<RenameChange, NamespaceFailure> {
        let cancel = controls.cancel;
        let control = controls.cooperative;
        let mut recorded = false;
        let mut attempted = false;
        let mut succeeded = false;
        let pid = unsafe { libc::getpid() };
        let result = (|| {
            check_control(cancel, control)?;
            if read > crate::directory_compare::MAX_READ {
                return Err(invalid("Allowance own rename inválido"));
            }
            let seal = self.endpoints.seal;
            log.intent(seal)?;
            recorded = true;
            log.checkpoint(OwnPoint::IntentDurable)?;
            let mut spent = 0;
            for s in &self.scopes {
                spent += s.inventory.verify_observed(
                    s.parent,
                    s.name,
                    cancel,
                    control,
                    Some(read - spent),
                )?;
            }
            let e = &self.endpoints;
            let a = &self.scopes[e.from];
            let b = &self.scopes[e.to];
            let (source_parent, source, name) =
                a.inventory
                    .pin_sync_path(a.parent, a.name, e.source_index, cancel)?;
            let (gp, _, parent_name) =
                b.inventory
                    .pin_sync_path(b.parent, b.name, e.to_parent, cancel)?;
            let destination_parent = data(
                &gp,
                &parent_name,
                b.inventory.entries[e.to_parent].identity,
                true,
            )?;
            let target = c(e
                .to_relative
                .rsplit(|&b| b == b'/')
                .next()
                .ok_or_else(|| invalid("Destino vacío"))?)?;
            let source_name = c(name.as_bytes())?;
            match open(&destination_parent, OsStr::from_bytes(target.as_bytes())) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => return Err(error),
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "Destino ocupado; sin sobrescritura",
                    ));
                }
            }
            log.checkpoint(OwnPoint::BeforeSyscall)?;
            check_control(cancel, control)?;
            log.ready(seal)?;
            check(cancel)?;
            if pid != unsafe { libc::getpid() } {
                return Err(invalid("Owner PID cambió antes del syscall"));
            }
            stable(
                &source_parent,
                &name,
                &source,
                self.change.identity,
                self.change.before,
            )?;
            log.checkpoint(OwnPoint::Dispatch)?;
            check(cancel)?;
            attempted = true;
            if unsafe {
                libc::syscall(
                    libc::SYS_renameat2,
                    source_parent.as_raw_fd(),
                    source_name.as_ptr(),
                    destination_parent.as_raw_fd(),
                    target.as_ptr(),
                    libc::RENAME_NOREPLACE,
                )
            } != 0
            {
                return Err(io::Error::last_os_error());
            }
            succeeded = true;
            log.checkpoint(OwnPoint::RenameReturned)?;
            let mut change = self.reconcile_inner(read - spent, cancel, control, |plan| {
                log.checkpoint(OwnPoint::Verified)?;
                plan.sync_after(cancel, control)?;
                log.checkpoint(OwnPoint::DataSynced)?;
                log.ready(seal)?;
                log.confirmed(&plan.change)?;
                log.checkpoint(OwnPoint::Confirmed)
            })?;
            change.payload += spent;
            Ok(change)
        })();
        result.map_err(|cause| NamespaceFailure {
            cause,
            intent_recorded: recorded,
            syscall_attempted: attempted,
            syscall_succeeded: succeeded,
        })
    }
}

impl RenameChange {
    pub(crate) fn observation(&self) -> Observation {
        Observation {
            object: Object {
                identity: self.identity,
                mount: self.mount,
            },
            revision: self.after,
            content: self.content,
            metadata: self.metadata,
        }
    }
}
