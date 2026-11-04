use super::super::workspace_batch::{Layout, PlacementSlot};
use super::*;
use crate::{
    directory_sync_journal::Object,
    exdev_source::{Inventory, observed_identity},
};
use std::{ffi::OsStr, fs, sync::Arc};

/// Shared absolute-container pin. Cloning this handle opens no descriptor.
#[derive(Clone)]
pub struct WorkspaceParent(Arc<Directory>);
impl std::ops::Deref for WorkspaceParent {
    type Target = File;
    fn deref(&self) -> &File {
        &self.0.file
    }
}
impl WorkspaceParent {
    pub fn verify(&self) -> io::Result<()> {
        self.0.check()
    }
    pub fn path(&self) -> &Path {
        &self.0.path
    }
}

/// Historical mkdir/named-postimage/fsync/ACK facts, not atomic inode provenance,
/// native approval, lasting freshness or a reservation of descriptor slots.
pub struct CreatedWorkspace {
    inventory: Inventory,
    parent: Arc<Directory>,
    name: OsString,
    transaction: u32,
    seal: [u8; 32],
    fd_slots_observed: usize,
}
impl std::fmt::Debug for CreatedWorkspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreatedWorkspace")
            .field("transaction", &self.transaction)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}
impl CreatedWorkspace {
    /// O_PATH inventory pin; use verified synchronization, never fsync this FD.
    pub fn root(&self) -> &File {
        self.inventory.root()
    }
    pub fn parent(&self) -> &File {
        &self.parent.file
    }
    pub fn name(&self) -> &OsStr {
        &self.name
    }
    pub fn transaction(&self) -> u32 {
        self.transaction
    }
    pub fn seal(&self) -> [u8; 32] {
        self.seal
    }
    pub fn fd_slots_observed(&self) -> usize {
        self.fd_slots_observed
    }
    pub fn parent_handle(&self) -> WorkspaceParent {
        WorkspaceParent(Arc::clone(&self.parent))
    }
    pub fn into_parts(self) -> (Inventory, WorkspaceParent, OsString) {
        (self.inventory, WorkspaceParent(self.parent), self.name)
    }
    fn check(&self, cancel: &AtomicBool) -> io::Result<()> {
        self.parent.check()?;
        private_directory(self.inventory.root())?;
        self.inventory
            .verify_staging(&self.parent.file, &self.name, 0, cancel, None)?;
        self.parent.check()
    }
}
pub struct WorkspaceFailure {
    pub cause: io::Error,
    pub completed: Vec<CreatedWorkspace>,
    pub path: Option<PathBuf>,
    pub intent_acknowledged: bool,
    pub syscall_attempted: bool,
    pub syscall_succeeded: bool,
    pub identity_recorded: bool,
    pub materialized_acknowledged: bool,
}
impl std::fmt::Debug for WorkspaceFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkspaceFailure")
            .field("cause", &self.cause)
            .field("completed", &self.completed.len())
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for WorkspaceFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cause.fmt(f)
    }
}
impl std::error::Error for WorkspaceFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}
#[derive(Clone, Copy)]
pub(in crate::directory_sync_store) enum WorkspacePoint {
    Admitted,
    IntentWritten,
    IntentFileSynced,
    IntentAcknowledged,
    IntentDurable,
    BeforeCreate,
    Created,
    IdentityRecorded,
    RootSynced,
    NamespaceSynced,
    MaterializedAcknowledged,
    Ready,
}
fn object(file: &File) -> io::Result<Object> {
    Ok(Object {
        identity: observed_identity(&file.metadata()?)?,
        mount: node(file)?.mount,
    })
}
pub(super) fn absent(parent: &File, name: &OsStr) -> io::Result<()> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::zeroed();
    let result = unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            c(name.as_bytes())?.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if result == 0 {
        return Err(bad("Nombre de workspace ya existe"));
    }
    let e = io::Error::last_os_error();
    if e.raw_os_error() != Some(libc::ENOENT) {
        return Err(e);
    }
    Ok(())
}
fn available_fd_slots() -> io::Result<usize> {
    let mut limits = std::mem::MaybeUninit::<libc::rlimit>::zeroed();
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, limits.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let limit = unsafe { limits.assume_init() }.rlim_cur;
    let mut occupied = 0u64;
    for entry in fs::read_dir("/proc/self/fd")? {
        let name = entry?.file_name();
        let fd: u64 = name
            .to_str()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| bad("Tabla FD inválida"))?;
        if fd < limit {
            occupied += 1;
        }
    }
    usize::try_from(limit.saturating_sub(occupied)).map_err(|_| bad("Límite FD no representable"))
}
impl Journal<'_> {
    /// All parents must already be owned0700, absolute, disjoint from both inputs
    /// and Store, and on their transaction's mount. No automatic cleanup/retry.
    pub fn create_workspaces(
        &mut self,
        physical: &Physical<'_, '_>,
        parents: &[&Path],
        cancel: &AtomicBool,
    ) -> Result<Vec<CreatedWorkspace>, WorkspaceFailure> {
        self.create_workspaces_with(physical, parents, cancel, |_| Ok(()))
    }
    pub fn create_workspaces_controlled(
        &mut self,
        physical: &Physical<'_, '_>,
        parents: &[&Path],
        control: &crate::copy_engine::Control,
    ) -> Result<Vec<CreatedWorkspace>, WorkspaceFailure> {
        self.create_workspaces_inner(
            physical,
            parents,
            control.cancellation_flag(),
            Some(control),
            |_| Ok(()),
        )
    }
    pub(in crate::directory_sync_store) fn create_workspaces_with(
        &mut self,
        physical: &Physical<'_, '_>,
        parents: &[&Path],
        cancel: &AtomicBool,
        hook: impl FnMut(WorkspacePoint) -> io::Result<()>,
    ) -> Result<Vec<CreatedWorkspace>, WorkspaceFailure> {
        self.create_workspaces_inner(physical, parents, cancel, None, hook)
    }
    fn create_workspaces_inner(
        &mut self,
        physical: &Physical<'_, '_>,
        parents: &[&Path],
        cancel: &AtomicBool,
        control: Option<&crate::copy_engine::Control>,
        mut hook: impl FnMut(WorkspacePoint) -> io::Result<()>,
    ) -> Result<Vec<CreatedWorkspace>, WorkspaceFailure> {
        let checkpoint = || -> io::Result<()> {
            if let Some(c) = control {
                c.checkpoint()?;
            }
            check_cancel(cancel)
        };
        let mut completed = Vec::new();
        let mut current_path = None;
        let mut writing = false;
        let (mut intent, mut attempted, mut succeeded, mut recorded, mut materialized) =
            (false, false, false, false, false);
        let result = (|| -> io::Result<()> {
            checkpoint()?;
            self.ensure_current()?;
            let admission = self.schema.staging_admission()?;
            if admission.physical_seal() != physical.seal()? {
                return Err(bad("Lote físico de workspace distinto"));
            }
            self.replay
                .state
                .workspace_batch(&self.schema, parents.len())?;
            // A point-in-time admission, NOT exclusion of other FD consumers.
            // Reserve no slots. Includes a conservative recursive/transient margin.
            let layout = Layout::describe(admission, parents)?;
            completed
                .try_reserve_exact(parents.len())
                .map_err(|_| io::Error::from(io::ErrorKind::OutOfMemory))?;
            let available = available_fd_slots()?;
            let margin = 4 * crate::directory_sync_admission::MAX_SCOPE_DEPTH + 32;
            if available
                .checked_sub(margin)
                .is_none_or(|n| n < layout.report.retained_fd_slots())
            {
                return Err(bad("Headroom FD insuficiente"));
            }
            let directories = layout.open(physical, &self.store.directory, &checkpoint)?;
            let mut placements = Vec::<PlacementSlot>::with_capacity(parents.len());
            for i in 0..parents.len() {
                checkpoint()?;
                let directory = layout.parent(&directories, i);
                let name = super::super::placements::workspace_name(self.schema.proof(), i);
                let seal = self.replay.state.workspace_spec(
                    &self.schema,
                    i as u32,
                    object(&directory.file)?,
                    name.as_bytes(),
                )?;
                absent(&directory.file, &name)?;
                placements.push((directory, name, seal));
            }
            drop(directories);
            // Every placement is checked before the first durable effect intent.
            for (directory, name, _) in &placements {
                directory.check()?;
                absent(&directory.file, name)?;
            }
            self.ensure_current()?;
            hook(WorkspacePoint::Admitted)?;
            // The complete placement artifact is durable before any workspace intent.
            writing = true;
            let registry = PlacementRegistry::create_with(
                self.store,
                &self.schema,
                &placements,
                cancel,
                |_| checkpoint(),
            )?;
            let registry_seal = registry.seal();
            self.placements = Some(registry);
            self.append(
                Event::PlacementRegistered {
                    seal: registry_seal,
                },
                cancel,
            )?;
            self.ensure_current()?;
            for (i, (directory, name, seal)) in placements.into_iter().enumerate() {
                (intent, attempted, succeeded, recorded, materialized) =
                    (false, false, false, false, false);
                current_path = Some(directory.path.join(&name));
                checkpoint()?;
                directory.check()?;
                absent(&directory.file, &name)?;
                self.ensure_current()?;
                self.replay.state.check(
                    &self.schema,
                    Event::WorkspaceCreateIntent {
                        transaction: i as u32,
                        seal,
                    },
                )?;
                writing = true;
                self.append_with(
                    Event::WorkspaceCreateIntent {
                        transaction: i as u32,
                        seal,
                    },
                    cancel,
                    |p| match p {
                        Barrier::Write => hook(WorkspacePoint::IntentWritten),
                        Barrier::FileSynced => hook(WorkspacePoint::IntentFileSynced),
                        Barrier::Acknowledged => hook(WorkspacePoint::IntentAcknowledged),
                        _ => Ok(()),
                    },
                )?;
                intent = true;
                hook(WorkspacePoint::IntentDurable)?;
                directory.check()?;
                absent(&directory.file, &name)?;
                hook(WorkspacePoint::BeforeCreate)?;
                checkpoint()?;
                directory.check()?;
                self.ensure_current()?;
                if self.replay.state.workspace_create_guard() != Some((i as u32, seal)) {
                    return Err(bad("Guard de workspace distinto"));
                }
                attempted = true;
                if unsafe {
                    libc::mkdirat(
                        directory.file.as_raw_fd(),
                        c(name.as_bytes())?.as_ptr(),
                        0o700,
                    )
                } != 0
                {
                    return Err(io::Error::last_os_error());
                }
                succeeded = true;
                hook(WorkspacePoint::Created)?;
                let inventory =
                    crate::exdev_source::capture_namespace(&directory.file, &name, 0, cancel)?;
                private_directory(inventory.root())?;
                if inventory.entries().len() != 1 {
                    return Err(bad("Workspace nuevo no está vacío"));
                }
                let workspace = CreatedWorkspace {
                    inventory,
                    parent: directory,
                    name,
                    transaction: i as u32,
                    seal,
                    fd_slots_observed: available,
                };
                workspace.check(cancel)?;
                self.append(
                    Event::WorkspaceCreated {
                        transaction: i as u32,
                        object: object(workspace.root())?,
                    },
                    cancel,
                )?;
                recorded = true;
                hook(WorkspacePoint::IdentityRecorded)?;
                checkpoint()?;
                workspace.inventory.sync_staging(
                    &workspace.parent.file,
                    &workspace.name,
                    cancel,
                    control,
                )?;
                hook(WorkspacePoint::RootSynced)?;
                workspace.parent.sync_namespace(&mut |_| checkpoint())?;
                hook(WorkspacePoint::NamespaceSynced)?;
                workspace.check(cancel)?;
                self.append(Event::WorkspaceMaterialized(i as u32), cancel)?;
                materialized = true;
                completed.push(workspace);
                hook(WorkspacePoint::MaterializedAcknowledged)?;
                completed.last().expect("workspace").check(cancel)?;
            }
            checkpoint()?;
            hook(WorkspacePoint::Ready)?;
            for workspace in &completed {
                workspace.check(cancel)?;
            }
            self.ensure_current()
        })();
        match result {
            Ok(()) => Ok(completed),
            Err(cause) => {
                if writing {
                    self.state = State::Uncertain;
                }
                Err(WorkspaceFailure {
                    cause,
                    completed,
                    path: current_path,
                    intent_acknowledged: intent,
                    syscall_attempted: attempted,
                    syscall_succeeded: succeeded,
                    identity_recorded: recorded,
                    materialized_acknowledged: materialized,
                })
            }
        }
    }
}
