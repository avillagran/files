//! Trusted staging coordination only. No destination publication or native approval.
use super::*;
use crate::{
    directory_sync_journal::Object,
    directory_sync_manifest::{NamespaceSnapshot, Prepared},
    exdev_source::{Inventory, NamespaceScope},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StageBatchReport {
    seal: [u8; 32],
    sources: [u64; 2],
    staged: u64,
    walked: u64,
    copied: u64,
    total: u64,
    steps: usize,
}
impl StageBatchReport {
    pub fn inspect(physical: &Physical<'_, '_>, parents: &[&Path]) -> io::Result<Self> {
        WorkspaceBatchReport::inspect(physical, parents)?;
        let view = physical.plan().record().view()?;
        let mut sources = [0u64; 2];
        for (s, root) in view.roots.iter().enumerate() {
            let mut seen = BTreeSet::new();
            for e in &root.entries {
                if e.identity.kind == Kind::File
                    && seen.insert((e.identity.device, e.identity.inode))
                {
                    sources[s] = add(sources[s], e.revision.size)?;
                }
            }
        }
        let (mut staged, mut copied) = (0, 0);
        for (i, n) in physical.nodes().iter().enumerate() {
            let l = n.source();
            let e = &view.roots[l.side].entries[l.index];
            if e.identity.kind == Kind::File {
                staged = add(staged, e.revision.size)?;
                if n.cohort()
                    .is_none_or(|g| physical.cohorts()[g].members()[0] == i)
                {
                    copied = add(copied, e.revision.size)?;
                }
            }
        }
        let walked = (0..physical.transactions().len())
            .map(|t| walk_bytes(physical, &view, t))
            .try_fold(0, |n, v| add(n, v?))?;
        let steps = physical.stage_metadata_order()?.len();
        let max = crate::directory_compare::MAX_READ;
        if sources.iter().any(|&n| n > max) || walked.checked_mul(2).is_none_or(|n| n > max) {
            return Err(bad("Lote excede las cuotas por pasada"));
        }
        // Two original-source snapshots, copy leaders, verify every staged file,
        // initial staging capture, two full walks per metadata step, final barrier.
        let multiplier = (steps as u64)
            .checked_mul(2)
            .and_then(|n| n.checked_add(3))
            .ok_or_else(|| bad("Lecturas de lote excedidas"))?;
        let total = add(
            add(sources[0], sources[1])?
                .checked_mul(2)
                .ok_or_else(|| bad("Lecturas de fuente excedidas"))?,
            add(
                add(copied, staged)?,
                walked
                    .checked_mul(multiplier)
                    .ok_or_else(|| bad("Lecturas staging excedidas"))?,
            )?,
        )?;
        Ok(Self {
            seal: physical.seal()?,
            sources,
            staged,
            walked,
            copied,
            total,
            steps,
        })
    }
    pub fn required_regular_bytes(&self) -> u64 {
        self.total
    }
    pub fn copied_regular_bytes(&self) -> u64 {
        self.copied
    }
    pub fn staged_regular_bytes(&self) -> u64 {
        self.staged
    }
    pub fn workspace_walk_regular_bytes(&self) -> u64 {
        self.walked
    }
    pub fn metadata_steps(&self) -> usize {
        self.steps
    }
    pub fn require_budget(&self, limit: u64) -> io::Result<()> {
        if limit < self.total {
            Err(bad("Presupuesto acumulado de staging insuficiente"))
        } else {
            Ok(())
        }
    }
}
fn add(a: u64, b: u64) -> io::Result<u64> {
    a.checked_add(b)
        .ok_or_else(|| bad("Contador de lecturas excedido"))
}
struct Reads {
    limit: u64,
    reserved: u64,
}
impl Reads {
    fn reserve(&mut self, amount: u64) -> io::Result<()> {
        let n = add(self.reserved, amount)?;
        if n > self.limit {
            return Err(bad("Presupuesto de lectura agotado"));
        }
        self.reserved = n;
        Ok(())
    }
    fn exact(observed: u64, expected: u64) -> io::Result<()> {
        if observed != expected {
            Err(bad("Lectura real no coincide con forecast"))
        } else {
            Ok(())
        }
    }
}
/// Historical absolute container/name/object binding; no approval or lasting freshness.
#[derive(Clone)]
pub struct WorkspaceLocation {
    parent: WorkspaceParent,
    name: OsString,
    object: Object,
}
impl WorkspaceLocation {
    pub fn parent(&self) -> &File {
        &self.parent
    }
    pub fn name(&self) -> &OsStr {
        &self.name
    }
    pub fn verify(&self) -> io::Result<()> {
        self.parent.verify()?;
        let named = owned(unsafe {
            libc::openat(
                self.parent.as_raw_fd(),
                c(self.name.as_bytes())?.as_ptr(),
                libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })?;
        private_directory(&named)?;
        if object(&named)? != self.object {
            return Err(bad("Nombre absoluto de workspace cambió"));
        }
        self.parent.verify()
    }
}
fn object(file: &File) -> io::Result<Object> {
    Ok(Object {
        identity: crate::exdev_source::observed_identity(&file.metadata()?)?,
        mount: node(file)?.mount,
    })
}
pub struct StagedBatch {
    inventories: Vec<Inventory>,
    locations: Vec<WorkspaceLocation>,
    sources: [NamespaceSnapshot; 2],
    report: StageBatchReport,
    barrier: StagingReceipt,
}
impl std::fmt::Debug for StagedBatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StagedBatch")
            .field("workspaces", &self.inventories.len())
            .field("report", &self.report)
            .finish_non_exhaustive()
    }
}
impl StagedBatch {
    pub fn report(&self) -> StageBatchReport {
        self.report
    }
    pub fn barrier(&self) -> &StagingReceipt {
        &self.barrier
    }
    pub fn inventories(&self) -> &[Inventory] {
        &self.inventories
    }
    pub fn locations(&self) -> &[WorkspaceLocation] {
        &self.locations
    }
    pub fn into_parts(
        self,
    ) -> (
        Vec<Inventory>,
        Vec<WorkspaceLocation>,
        [NamespaceSnapshot; 2],
        StagingReceipt,
    ) {
        (self.inventories, self.locations, self.sources, self.barrier)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageBatchPhase {
    Admission,
    SourceBefore(usize),
    Workspaces,
    Create(u32),
    Capture(usize),
    Metadata(u32),
    SourceAfter(usize),
    Verify(u32),
    Barrier,
    Return,
}
pub struct StageBatchFailure {
    pub cause: Box<dyn std::error::Error + Send + Sync>,
    pub phase: StageBatchPhase,
    pub reserved_regular_bytes: u64,
    pub private_effects_possible: bool,
    pub inventories: Vec<Inventory>,
    pub locations: Vec<WorkspaceLocation>,
}
impl std::fmt::Debug for StageBatchFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StageBatchFailure")
            .field("cause", &self.cause)
            .field("phase", &self.phase)
            .field("private_effects_possible", &self.private_effects_possible)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for StageBatchFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cause.fmt(f)
    }
}
impl std::error::Error for StageBatchFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.cause.as_ref())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageBatchPoint {
    Admitted,
    SourcesCaptured,
    WorkspacesCreated,
    BeforeNode(u32),
    NodeMaterialized(u32),
    Captured,
    BeforeMetadata(u32),
    MetadataMutation(u32),
    MetadataApplied(u32),
    SourcesRechecked,
    BeforeBarrier,
    Acknowledged,
    BeforeReturn,
}
fn locations(locations: &[WorkspaceLocation]) -> io::Result<()> {
    for l in locations {
        l.verify()?;
    }
    Ok(())
}
impl Journal<'_> {
    /// Explicit trusted coordinator: complete private staging, never publication.
    pub fn stage_batch(
        &mut self,
        physical: &Physical<'_, '_>,
        prepared: &Prepared,
        parents: &[&Path],
        total_regular_limit: u64,
        cancel: &AtomicBool,
    ) -> Result<StagedBatch, StageBatchFailure> {
        self.stage_batch_inner(
            physical,
            prepared,
            parents,
            total_regular_limit,
            cancel,
            None,
            |_| Ok(()),
        )
    }
    pub fn stage_batch_controlled(
        &mut self,
        physical: &Physical<'_, '_>,
        prepared: &Prepared,
        parents: &[&Path],
        total_regular_limit: u64,
        control: &crate::copy_engine::Control,
    ) -> Result<StagedBatch, StageBatchFailure> {
        self.stage_batch_inner(
            physical,
            prepared,
            parents,
            total_regular_limit,
            control.cancellation_flag(),
            Some(control),
            |_| Ok(()),
        )
    }
    pub fn stage_batch_observed(
        &mut self,
        physical: &Physical<'_, '_>,
        prepared: &Prepared,
        parents: &[&Path],
        total_regular_limit: u64,
        control: &crate::copy_engine::Control,
        observer: impl FnMut(StageBatchPoint) -> io::Result<()>,
    ) -> Result<StagedBatch, StageBatchFailure> {
        self.stage_batch_inner(
            physical,
            prepared,
            parents,
            total_regular_limit,
            control.cancellation_flag(),
            Some(control),
            observer,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::directory_sync_store) fn stage_batch_inner(
        &mut self,
        physical: &Physical<'_, '_>,
        prepared: &Prepared,
        parents: &[&Path],
        total_regular_limit: u64,
        cancel: &AtomicBool,
        control: Option<&crate::copy_engine::Control>,
        mut hook: impl FnMut(StageBatchPoint) -> io::Result<()>,
    ) -> Result<StagedBatch, StageBatchFailure> {
        type Error = Box<dyn std::error::Error + Send + Sync>;
        let checkpoint = || -> io::Result<()> {
            if let Some(c) = control {
                c.checkpoint()?;
            }
            check_cancel(cancel)
        };
        let mut phase = StageBatchPhase::Admission;
        let mut touched = false;
        let mut reads = Reads {
            limit: total_regular_limit,
            reserved: 0,
        };
        let mut inventories = Vec::new();
        let mut locs = Vec::new();
        let result = (|| -> Result<_, Error> {
            checkpoint()?;
            self.ensure_current()?;
            self.replay
                .state
                .workspace_batch(&self.schema, parents.len())?;
            let report = StageBatchReport::inspect(physical, parents)?;
            report.require_budget(total_regular_limit)?;
            if report.seal != self.schema.staging_admission()?.physical_seal()
                || prepared.record()?.bytes() != physical.plan().record().bytes()
            {
                return Err(bad("Preparación de lote distinta").into());
            }
            // Real parent/absence admission is read-only and precedes source hashing.
            {
                let admission = physical.staging_admission()?;
                let layout = super::super::workspace_batch::Layout::describe(&admission, parents)?;
                let dirs = layout.open(physical, &self.store.directory, &checkpoint)?;
                for i in 0..parents.len() {
                    checkpoint()?;
                    let dir = layout.parent(&dirs, i);
                    let name = super::super::placements::workspace_name(self.schema.proof(), i);
                    self.replay.state.workspace_spec(
                        &self.schema,
                        i as u32,
                        object(&dir.file)?,
                        name.as_bytes(),
                    )?;
                    super::workspace::absent(&dir.file, &name)?;
                }
            }
            hook(StageBatchPoint::Admitted)?;
            let mut snapshot = |side: usize| -> Result<NamespaceSnapshot, Error> {
                checkpoint()?;
                phase = StageBatchPhase::SourceBefore(side);
                reads.reserve(report.sources[side])?;
                let source =
                    snapshot_source(prepared, side, report.sources[side], cancel, control)?;
                Reads::exact(source.payload_read, report.sources[side])?;
                Ok(source)
            };
            let sources = [snapshot(0)?, snapshot(1)?];
            hook(StageBatchPoint::SourcesCaptured)?;
            phase = StageBatchPhase::Workspaces;
            let roots = match self.create_workspaces_control(physical, parents, cancel, control) {
                Ok(roots) => {
                    touched = true;
                    roots
                }
                Err(error) => {
                    touched = self.state == State::Uncertain;
                    return Err(Box::new(error));
                }
            };
            locs = roots
                .iter()
                .map(|root| {
                    Ok(WorkspaceLocation {
                        parent: root.parent_handle(),
                        name: root.name().to_owned(),
                        object: object(root.root())?,
                    })
                })
                .collect::<io::Result<Vec<_>>>()?;
            hook(StageBatchPoint::WorkspacesCreated)?;
            locations(&locs)?;
            let view = physical.plan().record().view()?;
            let mut leaders = BTreeMap::<usize, CreatedStageNode>::new();
            for (i, n) in physical.nodes().iter().enumerate() {
                checkpoint()?;
                phase = StageBatchPhase::Create(i as u32);
                let l = n.source();
                let e = &view.roots[l.side].entries[l.index];
                let group = n.cohort();
                let first = group.map(|g| physical.cohorts()[g].members()[0]);
                let last = group.map(|g| *physical.cohorts()[g].members().last().expect("cohort"));
                let alias = first.is_some_and(|first| first < i);
                let size = if e.identity.kind == Kind::File {
                    e.revision.size
                } else {
                    0
                };
                let expected = size
                    .checked_mul(if alias { 1 } else { 2 })
                    .ok_or_else(|| bad("Lecturas de nodo excedidas"))?;
                reads.reserve(expected)?;
                let leader = if alias {
                    Some(
                        leaders
                            .get(&group.expect("group"))
                            .ok_or_else(|| bad("Falta líder del coordinador"))?,
                    )
                } else {
                    None
                };
                let t = n.transaction();
                locs[t].verify()?;
                let receipt = self.create_stage_inner(
                    i as u32,
                    &sources[l.side],
                    roots[t].root(),
                    leader,
                    size,
                    cancel,
                    control,
                    |at| {
                        if matches!(at, super::create::Point::BeforeCreate) {
                            hook(StageBatchPoint::BeforeNode(i as u32))?;
                        }
                        locs[t].verify()
                    },
                )?;
                Reads::exact(add(receipt.copied(), receipt.verified())?, expected)?;
                hook(StageBatchPoint::NodeMaterialized(i as u32))?;
                locs[t].verify()?;
                if let Some(g) = group {
                    if first == Some(i) && last.is_some_and(|last| last > i) {
                        leaders.insert(g, receipt);
                    } else if last == Some(i) {
                        leaders.remove(&g);
                    }
                }
            }
            if !leaders.is_empty() {
                return Err(bad("Recibos de líderes no liberados").into());
            }
            drop(sources);
            for (t, root) in roots.into_iter().enumerate() {
                checkpoint()?;
                phase = StageBatchPhase::Capture(t);
                let bytes = walk_bytes(physical, &view, t)?;
                reads.reserve(bytes)?;
                let (empty, parent, name) = root.into_parts();
                drop(empty);
                locs[t].verify()?;
                let inventory = if let Some(control) = control {
                    crate::exdev_source::capture_namespace_controlled(
                        &parent, &name, bytes, control,
                    )?
                } else {
                    crate::exdev_source::capture_namespace(&parent, &name, bytes, cancel)?
                };
                Reads::exact(inventory.payload_read(), bytes)?;
                inventories.push(inventory);
            }
            hook(StageBatchPoint::Captured)?;
            locations(&locs)?;
            for i in physical.stage_metadata_order()? {
                checkpoint()?;
                phase = StageBatchPhase::Metadata(i as u32);
                hook(StageBatchPoint::BeforeMetadata(i as u32))?;
                locations(&locs)?;
                let bytes = report
                    .walked
                    .checked_mul(2)
                    .ok_or_else(|| bad("Lecturas metadata excedidas"))?;
                reads.reserve(bytes)?;
                let scopes = inventories
                    .iter_mut()
                    .zip(&locs)
                    .map(|(inventory, l)| NamespaceScope {
                        inventory,
                        parent: &l.parent,
                        name: &l.name,
                    })
                    .collect();
                let receipt =
                    self.stage_metadata_inner(i as u32, scopes, bytes, cancel, control, |at| {
                        if matches!(at, super::metadata::MetadataPoint::BeforeMutation) {
                            hook(StageBatchPoint::MetadataMutation(i as u32))?;
                            locations(&locs)?;
                        }
                        Ok(())
                    })?;
                Reads::exact(receipt.payload_read(), bytes)?;
                hook(StageBatchPoint::MetadataApplied(i as u32))?;
                locations(&locs)?;
            }
            for (i, n) in physical.nodes().iter().enumerate() {
                checkpoint()?;
                phase = StageBatchPhase::Verify(i as u32);
                // This appends one observation only; global location guards also
                // run before/after the barrier, without an extra N*T sweep here.
                locs[n.transaction()].verify()?;
                let observed = self.schema.batch_observation(
                    i,
                    &inventories[n.transaction()],
                    &locs[n.transaction()].name,
                )?;
                self.append(
                    Event::NodeVerified {
                        node: i as u32,
                        observed,
                    },
                    cancel,
                )?;
            }
            let mut snapshot = |side: usize| -> Result<NamespaceSnapshot, Error> {
                checkpoint()?;
                phase = StageBatchPhase::SourceAfter(side);
                reads.reserve(report.sources[side])?;
                let source =
                    snapshot_source(prepared, side, report.sources[side], cancel, control)?;
                Reads::exact(source.payload_read, report.sources[side])?;
                Ok(source)
            };
            let sources = [snapshot(0)?, snapshot(1)?];
            hook(StageBatchPoint::SourcesRechecked)?;
            phase = StageBatchPhase::Barrier;
            checkpoint()?;
            hook(StageBatchPoint::BeforeBarrier)?;
            locations(&locs)?;
            let bytes = report
                .walked
                .checked_mul(2)
                .ok_or_else(|| bad("Lecturas barrera excedidas"))?;
            reads.reserve(bytes)?;
            let scopes: Vec<_> = inventories
                .iter()
                .zip(&locs)
                .map(|(inventory, l)| StagingWorkspace {
                    inventory,
                    parent: &l.parent,
                    name: &l.name,
                })
                .collect();
            let barrier = if let Some(control) = control {
                self.confirm_staging_controlled(&scopes, bytes, control)?
            } else {
                self.confirm_staging(&scopes, bytes, cancel)?
            };
            Reads::exact(barrier.payload_read(), bytes)?;
            Reads::exact(reads.reserved, report.total)?;
            hook(StageBatchPoint::Acknowledged)?;
            phase = StageBatchPhase::Return;
            hook(StageBatchPoint::BeforeReturn)?;
            locations(&locs)?;
            self.ensure_current()?;
            Ok((sources, report, barrier))
        })();
        match result {
            Ok((sources, report, barrier)) => Ok(StagedBatch {
                inventories,
                locations: locs,
                sources,
                report,
                barrier,
            }),
            Err(cause) => {
                if touched {
                    self.state = State::Uncertain;
                }
                Err(StageBatchFailure {
                    cause,
                    phase,
                    reserved_regular_bytes: reads.reserved,
                    private_effects_possible: touched,
                    inventories,
                    locations: locs,
                })
            }
        }
    }
    fn create_workspaces_control(
        &mut self,
        physical: &Physical<'_, '_>,
        parents: &[&Path],
        cancel: &AtomicBool,
        control: Option<&crate::copy_engine::Control>,
    ) -> Result<Vec<CreatedWorkspace>, WorkspaceFailure> {
        if let Some(control) = control {
            self.create_workspaces_controlled(physical, parents, control)
        } else {
            self.create_workspaces(physical, parents, cancel)
        }
    }
}

fn snapshot_source(
    prepared: &Prepared,
    side: usize,
    read: u64,
    cancel: &AtomicBool,
    control: Option<&crate::copy_engine::Control>,
) -> io::Result<NamespaceSnapshot> {
    if let Some(control) = control {
        prepared.namespace_snapshot_controlled(side, read, control)
    } else {
        prepared.namespace_snapshot(side, read, cancel)
    }
}

fn walk_bytes(
    physical: &Physical<'_, '_>,
    view: &crate::directory_sync_record::View<'_>,
    t: usize,
) -> io::Result<u64> {
    let mut seen = BTreeSet::new();
    let mut total = 0;
    for i in physical.transactions()[t].nodes() {
        let n = &physical.nodes()[i];
        let l = n.source();
        let e = &view.roots[l.side].entries[l.index];
        if e.identity.kind == Kind::File
            && seen.insert(n.cohort().map_or((false, i), |g| (true, g)))
        {
            total = add(total, e.revision.size)?;
        }
    }
    Ok(total)
}
