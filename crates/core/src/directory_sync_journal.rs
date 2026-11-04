//! Sequenced sync effect declarations. Replay checks order/identity/intent, not
//! filesystem truth, human approval or fresh durability. No user-data syscalls.
#[cfg(target_os = "linux")]
use crate::directory_sync_store::Store;
use crate::{
    directory_sync_metadata::{Target, Time, Values},
    directory_sync_physical::{Operation, Physical},
    directory_sync_store::{Proof, StoredProof},
    move_manifest::{Identity, Kind, Reader, Revision, put_identity, put_revision},
};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    io,
};
pub const MAX_EVENT: usize = 192;
pub const MAX_EVENTS: u64 = 16 * (2 * crate::directory_sync_physical::MAX_NODES) as u64
    + 8 * crate::directory_sync_record::MAX_ENTRIES as u64
    + 16;
pub const MAX_JOURNAL: u64 = 256 * 1024 * 1024;
pub const HEADER: usize = 8 + 212 + 32;
const MAGIC: &[u8; 8] = b"FLSJNL\0\x01";
fn bad(s: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, s)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Object {
    pub identity: Identity,
    pub mount: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Observation {
    pub object: Object,
    pub revision: Revision,
    pub content: [u8; 32],
    pub metadata: [u8; 32],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NamespaceAction {
    Preserve,
    Publish,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    PlacementRegistered {
        seal: [u8; 32],
    },
    WorkspaceCreateIntent {
        transaction: u32,
        seal: [u8; 32],
    },
    WorkspaceMaterialized(u32),
    StageMetadataIntent {
        node: u32,
        before: Observation,
        seal: [u8; 32],
    },
    StageMetadataApplied {
        node: u32,
        after: Observation,
    },
    NodeCreateIntent {
        node: u32,
        seal: [u8; 32],
    },
    NodeMaterialized(u32),
    NamespaceIntent {
        transaction: u32,
        action: NamespaceAction,
        seal: [u8; 32],
    },
    /// Caller declaration only; no public event can prove a native UI action.
    Approval {
        ticket: [u8; 16],
    },
    WorkspaceIntent(u32),
    WorkspaceCreated {
        transaction: u32,
        object: Object,
    },
    NodeCreated {
        node: u32,
        object: Object,
    },
    NodeVerified {
        node: u32,
        observed: Observation,
    },
    WorkspaceSynced(u32),
    StagingComplete,
    EffectIntent(u32),
    AbsenceChecked(u32),
    OldPreserved {
        transaction: u32,
        observed: Observation,
        scope: [u8; 32],
    },
    BackupSynced(u32),
    PublishIntent(u32),
    Published {
        transaction: u32,
        observed: Observation,
        scope: [u8; 32],
    },
    EffectSynced(u32),
    Committed(u32),
    DirectoryIntent {
        index: u32,
        before: Observation,
    },
    DirectoryChanged {
        index: u32,
        after: Observation,
    },
    DirectorySynced(u32),
    Completed,
    UndoBegin,
    UndoIntent {
        transaction: u32,
        current: Option<Observation>,
    },
    UndoneEffect {
        transaction: u32,
        restored: Option<Observation>,
        scope: [u8; 32],
    },
    UndoSynced(u32),
    DirectoryUndoIntent {
        index: u32,
        before: Observation,
    },
    DirectoryRestored {
        index: u32,
        after: Observation,
    },
    DirectoryUndoSynced(u32),
    Undone,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Prepared,
    Staging,
    Effects,
    Completed,
    Undoing,
    Undone,
}
#[derive(Clone, Copy)]
struct Meta {
    mode: u32,
    uid: u32,
    gid: u32,
    access: Time,
    modified: Time,
    attrs: [u8; 32],
}
impl Meta {
    fn of(v: &Values) -> Self {
        Self {
            mode: v.mode(),
            uid: v.uid(),
            gid: v.gid(),
            access: v.access(),
            modified: v.modified(),
            attrs: v.attributes_digest(),
        }
    }
    fn digest(self, time: Time) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(self.mode.to_le_bytes());
        h.update(self.uid.to_le_bytes());
        h.update(self.gid.to_le_bytes());
        h.update(self.access.seconds.to_le_bytes());
        h.update(self.access.nanos.to_le_bytes());
        h.update(time.seconds.to_le_bytes());
        h.update(i64::from(time.nanos).to_le_bytes());
        h.update(self.attrs);
        h.finalize().into()
    }
    fn matches(self, o: Observation, time: Time) -> bool {
        o.revision.mode == self.mode
            && o.revision.uid == self.uid
            && o.revision.gid == self.gid
            && mtime(o) == time
            && o.metadata == self.digest(time)
    }
}
fn mtime(o: Observation) -> Time {
    Time {
        seconds: o.revision.mtime_seconds,
        nanos: o.revision.mtime_nanos,
    }
}
fn valid(o: Observation) -> bool {
    o.object.identity.inode != 0
        && o.object.mount != 0
        && o.revision.links != 0
        && o.revision.mode & !0o7777 == 0
        && o.revision.mtime_nanos < 1_000_000_000
        && o.revision.ctime_nanos < 1_000_000_000
}
#[derive(Clone)]
struct NodeSpec {
    source: Observation,
    source_root: Object,
    source_index: usize,
    source_relative: Vec<u8>,
    object: Object,
    content: [u8; 32],
    metadata: [u8; 32],
    meta: Meta,
    size: Option<u64>,
    group: Option<usize>,
    links: Option<u64>,
    transaction: usize,
    parent: Option<usize>,
    relative: Vec<u8>,
}
#[derive(Clone)]
struct TxSpec {
    operation: Operation,
    device: u64,
    mount: u64,
    nodes: std::ops::Range<usize>,
    old: Option<Observation>,
    old_scope: [u8; 32],
    root: Object,
    target: Vec<u8>,
    parent_object: Object,
}
#[derive(Clone)]
struct DirSpec {
    object: Object,
    original_content: [u8; 32],
    old: Meta,
    new: Meta,
    after_children: bool,
}
/// Derived from the complete saved physical intent and checked stored values.
/// Holding it is not authorization or positive proof of any logged observation.
pub struct Schema {
    admission: Result<crate::directory_sync_admission::StagingAdmission, String>,
    proof: Proof,
    nodes: Vec<NodeSpec>,
    metadata_order: Vec<usize>,
    transactions: Vec<TxSpec>,
    directories: Vec<DirSpec>,
    protected: HashSet<(u64, u64)>,
}
fn id(o: Object) -> (u64, u64) {
    (o.identity.device, o.identity.inode)
}
fn observe(e: &crate::directory_sync_record::EntryView<'_>) -> Observation {
    Observation {
        object: Object {
            identity: e.identity,
            mount: e.mount,
        },
        revision: e.revision,
        content: e.content,
        metadata: e.metadata,
    }
}
fn semantic(h: &mut Sha256, path: &[u8], o: Observation) {
    h.update((path.len() as u32).to_le_bytes());
    h.update(path);
    h.update(o.object.identity.device.to_le_bytes());
    h.update(o.object.identity.inode.to_le_bytes());
    h.update([o.object.identity.kind as u8]);
    h.update(o.object.mount.to_le_bytes());
    let mut r = o.revision;
    r.ctime_seconds = 0;
    r.ctime_nanos = 0;
    let mut b = Vec::with_capacity(52);
    put_revision(&mut b, r);
    h.update(b);
    h.update(o.content);
    h.update(o.metadata);
}
impl Schema {
    pub(crate) fn protect(&mut self, device: u64, inode: u64) {
        self.protected.insert((device, inode));
    }
    #[cfg(target_os = "linux")]
    pub fn derive(store: &Store, physical: &Physical<'_, '_>) -> io::Result<Self> {
        let proof = store.proof().clone();
        let expected = StoredProof::decode(&proof.encode())?;
        store.require_proof(&expected)?;
        let view = physical.plan().record().view()?;
        // Strong intent match, not merely numeric row compatibility.
        let bytes = proof.encode();
        if bytes[72..104] != physical.plan().record().seal()
            || bytes[104..136] != physical.plan().seal()?
            || bytes[136..168] != physical.seal()?
        {
            return Err(bad("Preparación distinta del diario"));
        }
        let mut protected = HashSet::new();
        for root in &view.roots {
            for e in &root.entries {
                protected.insert((e.identity.device, e.identity.inode));
            }
            for a in &root.ancestors {
                protected.insert((a.device, a.inode));
            }
        }
        protected.insert(proof.directory_identity());
        protected.insert(proof.file_identity());
        let mut nodes = Vec::with_capacity(physical.nodes().len());
        for (i, n) in physical.nodes().iter().enumerate() {
            let l = n.source();
            let e = &view.roots[l.side].entries[l.index];
            let wanted = store.value(&expected, Target::Stage(i))?;
            let tx = &physical.transactions()[n.transaction()];
            nodes.push(NodeSpec {
                source: observe(e),
                source_root: observe(&view.roots[l.side].entries[0]).object,
                source_index: l.index,
                source_relative: e.relative.to_vec(),
                object: Object {
                    identity: Identity {
                        device: view.roots[tx.parent().side].entries[tx.parent().index]
                            .identity
                            .device,
                        inode: 1,
                        kind: e.identity.kind,
                    },
                    mount: tx.mount(),
                },
                content: e.content,
                metadata: wanted.values().evidence_digest(),
                meta: Meta::of(wanted.values()),
                size: if e.identity.kind == Kind::Directory {
                    None
                } else {
                    Some(e.revision.size)
                },
                group: n.cohort(),
                links: if e.identity.kind == Kind::Directory {
                    None
                } else {
                    Some(
                        n.cohort()
                            .map_or(1, |g| physical.cohorts()[g].members().len() as u64),
                    )
                },
                transaction: n.transaction(),
                parent: n.parent_node(),
                relative: {
                    let root = physical.nodes()
                        [physical.transactions()[n.transaction()].nodes().start]
                        .source();
                    let prefix = view.roots[root.side].entries[root.index].relative;
                    if e.relative == prefix {
                        Vec::new()
                    } else {
                        e.relative
                            .get(prefix.len() + 1..)
                            .ok_or_else(|| bad("Relative de nodo inválida"))?
                            .to_vec()
                    }
                },
            });
        }
        let mut transactions = Vec::with_capacity(physical.transactions().len());
        for t in physical.transactions() {
            let old = t
                .old()
                .map(|l| observe(&view.roots[l.side].entries[l.index]));
            let mut h = Sha256::new();
            h.update(b"Files:sync-old-scope:v1\0");
            for l in &physical.preserved_entries()[t.backup_entries()] {
                let e = &view.roots[l.side].entries[l.index];
                let root = physical
                    .plan()
                    .name(t.row())
                    .ok_or_else(|| bad("Fila ausente"))?;
                let path = if e.relative == root {
                    b"".as_slice()
                } else {
                    e.relative
                        .get(root.len() + 1..)
                        .ok_or_else(|| bad("Scope inválido"))?
                };
                semantic(&mut h, path, observe(e));
            }
            transactions.push(TxSpec {
                operation: t.operation(),
                device: view.roots[t.parent().side].entries[t.parent().index]
                    .identity
                    .device,
                mount: t.mount(),
                nodes: t.nodes(),
                old,
                old_scope: h.finalize().into(),
                root: observe(&view.roots[t.side()].entries[0]).object,
                parent_object: observe(&view.roots[t.parent().side].entries[t.parent().index])
                    .object,
                target: physical
                    .plan()
                    .name(t.row())
                    .ok_or_else(|| bad("Target ausente"))?
                    .to_vec(),
            });
        }
        let mut directories = Vec::with_capacity(physical.directory_metadata().len());
        for (i, d) in physical.directory_metadata().iter().enumerate() {
            let e = &view.roots[d.target().side].entries[d.target().index];
            let old = store.value(&expected, Target::UndoDirectory(i))?;
            let new = store.value(&expected, Target::Directory(i))?;
            directories.push(DirSpec {
                object: observe(e).object,
                original_content: e.content,
                old: Meta::of(old.values()),
                new: Meta::of(new.values()),
                after_children: new.after_children(),
            });
        }
        Ok(Self {
            admission: physical.staging_admission().map_err(|e| e.to_string()),
            proof,
            nodes,
            metadata_order: physical.stage_metadata_order()?,
            transactions,
            directories,
            protected,
        })
    }
    pub fn staging_admission(
        &self,
    ) -> io::Result<&crate::directory_sync_admission::StagingAdmission> {
        self.admission
            .as_ref()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.clone()))
    }
    pub fn proof(&self) -> &Proof {
        &self.proof
    }
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    pub fn transaction_count(&self) -> usize {
        self.transactions.len()
    }
    pub fn directory_count(&self) -> usize {
        self.directories.len()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Intent,
    Ready,
    Preserved,
    Publishing,
    Published,
    Synced,
}
#[derive(Clone, Copy)]
struct Pending {
    index: usize,
    step: Step,
    before: Option<Observation>,
}
pub struct State {
    phase: Phase,
    namespace: Option<(u32, NamespaceAction, [u8; 32])>,
    node_create: Option<(u32, [u8; 32])>,
    stage_meta: Option<(u32, Observation, [u8; 32])>,
    metadata_done: usize,
    metadata_tracked: bool,
    metadata_after: HashMap<(u64, u64), Observation>,
    ticket: Option<[u8; 16]>,
    placement_seal: Option<[u8; 32]>,
    workspace_create: Option<(u32, [u8; 32])>,
    workspace_pending: Option<usize>,
    workspaces: Vec<Object>,
    created: Vec<Object>,
    verified: Vec<Observation>,
    workspace_synced: usize,
    committed: usize,
    directories: usize,
    pending: Option<Pending>,
    directory_pending: Option<Pending>,
    published: Vec<Option<Observation>>,
    undo_remaining: usize,
    undo_directory: usize,
    seen: HashMap<(u64, u64), usize>,
    groups: HashMap<usize, Object>,
    workspace_ids: HashSet<(u64, u64)>,
}
impl State {
    pub fn new(_schema: &Schema) -> Self {
        Self {
            phase: Phase::Prepared,
            namespace: None,
            node_create: None,
            stage_meta: None,
            metadata_done: 0,
            metadata_tracked: false,
            metadata_after: HashMap::new(),
            ticket: None,
            placement_seal: None,
            workspace_create: None,
            workspace_pending: None,
            workspaces: Vec::new(),
            created: Vec::new(),
            verified: Vec::new(),
            workspace_synced: 0,
            committed: 0,
            directories: 0,
            pending: None,
            directory_pending: None,
            published: Vec::new(),
            undo_remaining: 0,
            undo_directory: 0,
            seen: HashMap::new(),
            groups: HashMap::new(),
            workspace_ids: HashSet::new(),
        }
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn committed(&self) -> usize {
        self.committed
    }
    pub fn directories(&self) -> usize {
        self.directories
    }
    pub fn pending_transaction(&self) -> Option<usize> {
        self.pending.map(|p| p.index)
    }
    pub fn pending_directory(&self) -> Option<usize> {
        self.directory_pending.map(|p| p.index)
    }
    pub fn pending_step(&self) -> Option<(usize, Step)> {
        self.pending.map(|p| (p.index, p.step))
    }
    pub fn pending_directory_step(&self) -> Option<(usize, Step)> {
        self.directory_pending.map(|p| (p.index, p.step))
    }
    fn root_matches(&self, s: &Schema, t: usize, o: Observation) -> bool {
        s.transactions
            .get(t)
            .and_then(|v| self.verified.get(v.nodes.start))
            .is_some_and(|v| same_rename(*v, o))
    }
    pub fn staged_scope(&self, s: &Schema, t: usize) -> io::Result<[u8; 32]> {
        let spec = s
            .transactions
            .get(t)
            .ok_or_else(|| bad("Transacción ausente"))?;
        let mut h = Sha256::new();
        h.update(b"Files:sync-staged-scope:v1\0");
        for i in spec.nodes.clone() {
            let o = self
                .verified
                .get(i)
                .ok_or_else(|| bad("Nodo sin verificar"))?;
            h.update((i as u32).to_le_bytes());
            semantic(&mut h, b"", *o);
        }
        Ok(h.finalize().into())
    }
    pub(crate) fn check(&self, s: &Schema, e: Event) -> io::Result<()> {
        if let Some((i, _)) = self.workspace_create
            && !matches!(e, Event::WorkspaceCreated { transaction, .. } | Event::WorkspaceMaterialized(transaction) if transaction == i)
        {
            return Err(bad("Creación de workspace pendiente"));
        }
        let tx = |i: u32| {
            s.transactions
                .get(i as usize)
                .ok_or_else(|| bad("Transacción fuera de rango"))
        };
        let raw_pending = |i: u32, step: Step| {
            self.pending
                .is_some_and(|p| p.index == i as usize && p.step == step)
        };
        let pending = |i: u32, step: Step| self.phase == Phase::Effects && raw_pending(i, step);
        let undo_pending =
            |i: u32, step: Step| self.phase == Phase::Undoing && raw_pending(i, step);
        let directory = |i: u32, step: Step, undo: bool| {
            self.directory_pending
                .is_some_and(|p| p.index == i as usize && p.step == step)
                && self.phase == if undo { Phase::Undoing } else { Phase::Effects }
        };
        let ok = match e {
            Event::PlacementRegistered { seal } => {
                self.phase == Phase::Staging
                    && seal != [0; 32]
                    && self.placement_seal.is_none()
                    && self.workspaces.is_empty()
                    && self.workspace_pending.is_none()
            }
            Event::WorkspaceCreateIntent { transaction, seal } => {
                seal != [0; 32]
                    && self.workspace_create.is_none()
                    && self.phase == Phase::Staging
                    && self.workspace_pending.is_none()
                    && transaction as usize == self.workspaces.len()
                    && tx(transaction).is_ok()
            }
            Event::WorkspaceMaterialized(i) => {
                self.workspace_create.is_some_and(|(t, _)| t == i)
                    && self.workspace_pending.is_none()
                    && self.workspaces.len() == i as usize + 1
            }

            Event::StageMetadataIntent { node, before, seal } => {
                self.phase == Phase::Staging
                    && self.node_create.is_none()
                    && self.stage_meta.is_none()
                    && self.workspace_pending.is_none()
                    && self.created.len() == s.nodes.len()
                    && self.verified.is_empty()
                    && s.metadata_order.get(self.metadata_done) == Some(&(node as usize))
                    && seal != [0; 32]
                    && valid(before)
                    && self.created.get(node as usize) == Some(&before.object)
                    && s.nodes.get(node as usize).is_some_and(|n| {
                        before.content == n.content
                            && n.size.is_none_or(|v| v == before.revision.size)
                            && n.links.is_none_or(|v| v == before.revision.links)
                    })
            }
            Event::StageMetadataApplied { node, after } => {
                self.phase == Phase::Staging
                    && valid(after)
                    && self.stage_meta.is_some_and(|(i, b, _)| {
                        i == node
                            && b.object == after.object
                            && b.revision.size == after.revision.size
                            && b.revision.links == after.revision.links
                    })
                    && s.nodes.get(node as usize).is_some_and(|n| {
                        after.content == n.content
                            && after.metadata == n.metadata
                            && n.meta.matches(after, n.meta.modified)
                    })
            }

            Event::NodeCreateIntent { node, seal } => {
                self.phase == Phase::Staging
                    && self.node_create.is_none()
                    && self.workspace_pending.is_none()
                    && self.workspaces.len() == s.transactions.len()
                    && self.verified.is_empty()
                    && node as usize == self.created.len()
                    && (node as usize) < s.nodes.len()
                    && seal != [0; 32]
            }
            Event::NodeMaterialized(node) => {
                self.phase == Phase::Staging
                    && self.node_create.is_some_and(|(i, _)| i == node)
                    && self.created.len() == node as usize + 1
                    && self.verified.is_empty()
            }

            Event::NamespaceIntent {
                transaction,
                action,
                seal,
            } => {
                self.namespace.is_none()
                    && seal != [0; 32]
                    && match action {
                        NamespaceAction::Preserve => {
                            pending(transaction, Step::Intent) && tx(transaction)?.old.is_some()
                        }
                        NamespaceAction::Publish => {
                            pending(transaction, Step::Publishing)
                                && tx(transaction)?.operation != Operation::Retire
                        }
                    }
            }
            Event::Approval { ticket } => self.phase == Phase::Prepared && ticket != [0; 16],
            Event::WorkspaceIntent(i) => {
                self.phase == Phase::Staging
                    && self.workspace_pending.is_none()
                    && i as usize == self.workspaces.len()
                    && tx(i).is_ok()
            }
            Event::WorkspaceCreated {
                transaction: i,
                object: o,
            } => {
                self.phase == Phase::Staging
                    && self.workspace_pending == Some(i as usize)
                    && tx(i).is_ok_and(|t| {
                        o.identity.kind == Kind::Directory
                            && o.identity.device == t.device
                            && o.mount == t.mount
                            && o.identity.inode != 0
                    })
                    && !s.protected.contains(&id(o))
                    && !self.workspace_ids.contains(&id(o))
            }
            Event::NodeCreated { node: i, object: o } => {
                self.phase == Phase::Staging
                    && self.workspaces.len() == s.transactions.len()
                    && self.workspace_pending.is_none()
                    && i as usize == self.created.len()
                    && self.node_create.is_none_or(|(node, _)| node == i)
                    && s.nodes.get(i as usize).is_some_and(|n| {
                        o.identity.kind == n.object.identity.kind
                            && o.identity.device == n.object.identity.device
                            && o.mount == n.object.mount
                            && o.identity.inode != 0
                            && n.transaction < self.workspaces.len()
                            && n.parent.is_none_or(|p| p < i as usize)
                            && n.group
                                .is_none_or(|g| self.groups.get(&g).is_none_or(|group| *group == o))
                    })
                    && !s.protected.contains(&id(o))
                    && !self.workspace_ids.contains(&id(o))
                    && self.seen.get(&id(o)).is_none_or(|&prior| {
                        s.nodes[prior].group.is_some()
                            && s.nodes[prior].group == s.nodes[i as usize].group
                    })
            }
            Event::NodeVerified {
                node: i,
                observed: o,
            } => {
                self.phase == Phase::Staging
                    && self.node_create.is_none()
                    && self.stage_meta.is_none()
                    && (!self.metadata_tracked || self.metadata_done == s.metadata_order.len())
                    && self.created.len() == s.nodes.len()
                    && i as usize == self.verified.len()
                    && valid(o)
                    && self.created.get(i as usize) == Some(&o.object)
                    && s.nodes.get(i as usize).is_some_and(|n| {
                        o.content == n.content
                            && o.metadata == n.metadata
                            && n.meta.matches(o, n.meta.modified)
                            && n.size.is_none_or(|size| size == o.revision.size)
                            && n.links.is_none_or(|links| links == o.revision.links)
                    })
            }
            Event::WorkspaceSynced(i) => {
                self.phase == Phase::Staging
                    && self.verified.len() == s.nodes.len()
                    && i as usize == self.workspace_synced
                    && (i as usize) < self.workspaces.len()
            }
            Event::StagingComplete => {
                self.phase == Phase::Staging
                    && self.workspace_pending.is_none()
                    && self.workspaces.len() == s.transactions.len()
                    && self.verified.len() == s.nodes.len()
                    && self.workspace_synced == self.workspaces.len()
            }
            Event::EffectIntent(i) => {
                self.phase == Phase::Effects
                    && self.pending.is_none()
                    && self.directory_pending.is_none()
                    && self.directories == 0
                    && i as usize == self.committed
                    && tx(i).is_ok()
            }
            Event::AbsenceChecked(i) => pending(i, Step::Intent) && tx(i)?.old.is_none(),
            Event::OldPreserved {
                transaction: i,
                observed: o,
                scope,
            } => {
                pending(i, Step::Intent)
                    && tx(i)?.old.is_some_and(|old| same_rename(old, o))
                    && scope == tx(i)?.old_scope
            }
            Event::BackupSynced(i) => pending(i, Step::Preserved),
            Event::PublishIntent(i) => {
                pending(i, Step::Ready) && tx(i)?.operation != Operation::Retire
            }
            Event::Published {
                transaction: i,
                observed: o,
                scope,
            } => {
                pending(i, Step::Publishing)
                    && self.root_matches(s, i as usize, o)
                    && scope == self.staged_scope(s, i as usize)?
            }
            Event::EffectSynced(i) => {
                pending(i, Step::Published)
                    || (pending(i, Step::Ready) && tx(i)?.operation == Operation::Retire)
            }
            Event::Committed(i) => pending(i, Step::Synced) && i as usize == self.committed,
            Event::DirectoryIntent {
                index: i,
                before: o,
            } => {
                self.phase == Phase::Effects
                    && self.committed == s.transactions.len()
                    && self.pending.is_none()
                    && self.directory_pending.is_none()
                    && i as usize == self.directories
                    && s.directories.get(i as usize).is_some_and(|d| {
                        valid(o) && o.object == d.object && d.old.matches(o, mtime(o))
                    })
            }
            Event::DirectoryChanged { index: i, after: o } => {
                directory(i, Step::Intent, false)
                    && s.directories.get(i as usize).is_some_and(|d| {
                        self.directory_pending
                            .and_then(|p| p.before)
                            .is_some_and(|before| {
                                metadata_change(
                                    before,
                                    o,
                                    d.new,
                                    if d.after_children {
                                        mtime(before)
                                    } else {
                                        d.new.modified
                                    },
                                )
                            })
                    })
            }
            Event::DirectorySynced(i) => directory(i, Step::Published, false),
            Event::Completed => {
                self.phase == Phase::Effects
                    && self.committed == s.transactions.len()
                    && self.directories == s.directories.len()
                    && self.pending.is_none()
                    && self.directory_pending.is_none()
            }
            Event::UndoBegin => {
                matches!(self.phase, Phase::Effects | Phase::Completed)
                    && self.pending.is_none()
                    && self.directory_pending.is_none()
            }
            Event::UndoIntent {
                transaction: i,
                current,
            } => {
                self.phase == Phase::Undoing
                    && self.pending.is_none()
                    && self.directory_pending.is_none()
                    && self.undo_remaining > 0
                    && i as usize == self.undo_remaining - 1
                    && match (self.published.get(i as usize).copied().flatten(), current) {
                        (None, None) => true,
                        (Some(a), Some(b)) => same_rename(a, b),
                        _ => false,
                    }
            }
            Event::UndoneEffect {
                transaction: i,
                restored,
                scope,
            } => {
                undo_pending(i, Step::Intent)
                    && match (tx(i)?.old, restored) {
                        (None, None) => scope == [0; 32],
                        (Some(a), Some(b)) => same_rename(a, b) && scope == tx(i)?.old_scope,
                        _ => false,
                    }
            }
            Event::UndoSynced(i) => undo_pending(i, Step::Published),
            Event::DirectoryUndoIntent {
                index: i,
                before: o,
            } => {
                self.phase == Phase::Undoing
                    && self.undo_remaining == 0
                    && self.pending.is_none()
                    && self.directory_pending.is_none()
                    && i as usize == self.undo_directory
                    && (i as usize) < self.directories
                    && s.directories.get(i as usize).is_some_and(|d| {
                        valid(o)
                            && o.object == d.object
                            && o.content == d.original_content
                            && d.new.matches(o, mtime(o))
                    })
            }
            Event::DirectoryRestored { index: i, after: o } => {
                directory(i, Step::Intent, true)
                    && s.directories.get(i as usize).is_some_and(|d| {
                        self.directory_pending
                            .and_then(|p| p.before)
                            .is_some_and(|before| metadata_change(before, o, d.old, d.old.modified))
                    })
            }
            Event::DirectoryUndoSynced(i) => directory(i, Step::Published, true),
            Event::Undone => {
                self.phase == Phase::Undoing
                    && self.undo_remaining == 0
                    && self.undo_directory == self.directories
                    && self.pending.is_none()
                    && self.directory_pending.is_none()
            }
        };
        if ok {
            Ok(())
        } else {
            Err(bad("Transición de diario inválida o fuera de orden"))
        }
    }
    pub(crate) fn apply(&mut self, s: &Schema, e: Event) {
        match e {
            Event::PlacementRegistered { seal } => self.placement_seal = Some(seal),
            Event::WorkspaceCreateIntent { transaction, seal } => {
                self.workspace_create = Some((transaction, seal));
                self.workspace_pending = Some(transaction as usize);
            }
            Event::WorkspaceMaterialized(_) => self.workspace_create = None,
            Event::StageMetadataIntent { node, before, seal } => {
                self.metadata_tracked = true;
                self.stage_meta = Some((node, before, seal));
            }
            Event::StageMetadataApplied { after, .. } => {
                self.metadata_after.insert(id(after.object), after);
                self.stage_meta = None;
                self.metadata_done += 1;
            }

            Event::NodeCreateIntent { node, seal } => self.node_create = Some((node, seal)),
            Event::NodeMaterialized(_) => self.node_create = None,

            Event::NamespaceIntent {
                transaction,
                action,
                seal,
            } => self.namespace = Some((transaction, action, seal)),
            Event::Approval { ticket } => {
                self.ticket = Some(ticket);
                self.phase = Phase::Staging;
            }
            Event::WorkspaceIntent(i) => self.workspace_pending = Some(i as usize),
            Event::WorkspaceCreated { object, .. } => {
                self.workspace_ids.insert(id(object));
                self.workspaces.push(object);
                self.workspace_pending = None;
            }
            Event::NodeCreated { node, object } => {
                self.seen.insert(id(object), node as usize);
                if let Some(g) = s.nodes[node as usize].group {
                    self.groups.insert(g, object);
                }
                self.created.push(object);
            }
            Event::NodeVerified { observed, .. } => self.verified.push(observed),
            Event::WorkspaceSynced(_) => self.workspace_synced += 1,
            Event::StagingComplete => self.phase = Phase::Effects,
            Event::EffectIntent(i) | Event::UndoIntent { transaction: i, .. } => {
                self.pending = Some(Pending {
                    index: i as usize,
                    step: Step::Intent,
                    before: None,
                })
            }
            Event::AbsenceChecked(_) | Event::BackupSynced(_) => {
                self.pending.as_mut().expect("checked").step = Step::Ready
            }
            Event::OldPreserved { .. } => {
                self.namespace = None;
                self.pending.as_mut().expect("checked").step = Step::Preserved
            }
            Event::PublishIntent(_) => {
                self.pending.as_mut().expect("checked").step = Step::Publishing
            }
            Event::Published { observed, .. } => {
                self.namespace = None;
                self.pending.as_mut().expect("checked").step = Step::Published;
                self.published.push(Some(observed));
            }
            Event::EffectSynced(i) => {
                self.pending.as_mut().expect("checked").step = Step::Synced;
                if self.published.len() == i as usize {
                    self.published.push(None);
                }
            }
            Event::Committed(_) => {
                self.committed += 1;
                self.pending = None;
            }
            Event::DirectoryIntent { index, before }
            | Event::DirectoryUndoIntent { index, before } => {
                self.directory_pending = Some(Pending {
                    index: index as usize,
                    step: Step::Intent,
                    before: Some(before),
                })
            }
            Event::DirectoryChanged { .. } | Event::DirectoryRestored { .. } => {
                self.directory_pending.as_mut().expect("checked").step = Step::Published
            }
            Event::DirectorySynced(_) => {
                self.directories += 1;
                self.directory_pending = None;
            }
            Event::Completed => self.phase = Phase::Completed,
            Event::UndoBegin => {
                self.phase = Phase::Undoing;
                self.undo_remaining = self.committed;
            }
            Event::UndoneEffect { .. } => {
                self.pending.as_mut().expect("checked").step = Step::Published
            }
            Event::UndoSynced(_) => {
                self.undo_remaining -= 1;
                self.pending = None;
            }
            Event::DirectoryUndoSynced(_) => {
                self.undo_directory += 1;
                self.directory_pending = None;
            }
            Event::Undone => self.phase = Phase::Undone,
        }
    }
    pub fn accept(&mut self, s: &Schema, e: Event) -> io::Result<()> {
        self.check(s, e)?;
        self.apply(s, e);
        Ok(())
    }
}
fn same_rename(a: Observation, b: Observation) -> bool {
    valid(b)
        && a.object == b.object
        && b.revision.matches_after_rename(a.revision)
        && a.content == b.content
        && a.metadata == b.metadata
}
fn metadata_change(before: Observation, after: Observation, meta: Meta, time: Time) -> bool {
    valid(after)
        && after.object == before.object
        && after.content == before.content
        && after.revision.size == before.revision.size
        && after.revision.links == before.revision.links
        && meta.matches(after, time)
}
fn put_object(out: &mut Vec<u8>, o: Object) {
    put_identity(out, o.identity);
    out.extend_from_slice(&o.mount.to_le_bytes());
}
fn put_observation(out: &mut Vec<u8>, o: Observation) {
    put_object(out, o.object);
    put_revision(out, o.revision);
    out.extend_from_slice(&o.content);
    out.extend_from_slice(&o.metadata);
}
fn object(r: &mut Reader<'_>) -> io::Result<Object> {
    let identity = r.identity().map_err(bad)?;
    let mount = r.u64().map_err(bad)?;
    if mount == 0 {
        return Err(bad("Montaje inválido"));
    }
    Ok(Object { identity, mount })
}
fn observation(r: &mut Reader<'_>) -> io::Result<Observation> {
    let o = Observation {
        object: object(r)?,
        revision: r.revision().map_err(bad)?,
        content: r.array().map_err(bad)?,
        metadata: r.array().map_err(bad)?,
    };
    if !valid(o) {
        return Err(bad("Observación inválida"));
    }
    Ok(o)
}
fn optional(out: &mut Vec<u8>, o: Option<Observation>) {
    out.push(u8::from(o.is_some()));
    if let Some(o) = o {
        put_observation(out, o);
    }
}
fn optional_read(r: &mut Reader<'_>) -> io::Result<Option<Observation>> {
    match r.take(1).map_err(bad)?[0] {
        0 => Ok(None),
        1 => Ok(Some(observation(r)?)),
        _ => Err(bad("Flag inválido")),
    }
}
pub fn encode_event(e: Event) -> Vec<u8> {
    let mut out = Vec::with_capacity(MAX_EVENT);
    let (tag, index) = match e {
        Event::PlacementRegistered { seal } => {
            out.push(34);
            out.extend_from_slice(&seal);
            return out;
        }
        Event::WorkspaceCreateIntent { transaction, .. } => (32, transaction),
        Event::WorkspaceMaterialized(transaction) => (33, transaction),
        Event::StageMetadataIntent { node, .. } => (30, node),
        Event::StageMetadataApplied { node, .. } => (31, node),
        Event::NamespaceIntent { transaction, .. } => (27, transaction),
        Event::Approval { ticket } => {
            out.push(0);
            out.extend_from_slice(&ticket);
            return out;
        }
        Event::NodeCreateIntent { node, .. } => (28, node),
        Event::NodeMaterialized(i) => (29, i),
        Event::WorkspaceIntent(i) => (1, i),
        Event::WorkspaceCreated { transaction: i, .. } => (2, i),
        Event::NodeCreated { node: i, .. } => (3, i),
        Event::NodeVerified { node: i, .. } => (4, i),
        Event::WorkspaceSynced(i) => (5, i),
        Event::StagingComplete => {
            out.push(6);
            return out;
        }
        Event::EffectIntent(i) => (7, i),
        Event::AbsenceChecked(i) => (8, i),
        Event::OldPreserved { transaction: i, .. } => (9, i),
        Event::BackupSynced(i) => (10, i),
        Event::PublishIntent(i) => (11, i),
        Event::Published { transaction: i, .. } => (12, i),
        Event::EffectSynced(i) => (13, i),
        Event::Committed(i) => (14, i),
        Event::DirectoryIntent { index: i, .. } => (15, i),
        Event::DirectoryChanged { index: i, .. } => (16, i),
        Event::DirectorySynced(i) => (17, i),
        Event::Completed => {
            out.push(18);
            return out;
        }
        Event::UndoBegin => {
            out.push(19);
            return out;
        }
        Event::UndoIntent { transaction: i, .. } => (20, i),
        Event::UndoneEffect { transaction: i, .. } => (21, i),
        Event::UndoSynced(i) => (22, i),
        Event::DirectoryUndoIntent { index: i, .. } => (23, i),
        Event::DirectoryRestored { index: i, .. } => (24, i),
        Event::DirectoryUndoSynced(i) => (25, i),
        Event::Undone => {
            out.push(26);
            return out;
        }
    };
    out.push(tag);
    out.extend_from_slice(&index.to_le_bytes());
    match e {
        Event::StageMetadataIntent { before, seal, .. } => {
            put_observation(&mut out, before);
            out.extend_from_slice(&seal);
        }
        Event::StageMetadataApplied { after, .. } => put_observation(&mut out, after),

        Event::NodeCreateIntent { seal, .. } | Event::WorkspaceCreateIntent { seal, .. } => {
            out.extend_from_slice(&seal)
        }

        Event::NamespaceIntent { action, seal, .. } => {
            out.push(match action {
                NamespaceAction::Preserve => 0,
                NamespaceAction::Publish => 1,
            });
            out.extend_from_slice(&seal);
        }
        Event::WorkspaceCreated { object, .. } | Event::NodeCreated { object, .. } => {
            put_object(&mut out, object)
        }
        Event::NodeVerified { observed, .. }
        | Event::DirectoryIntent {
            before: observed, ..
        }
        | Event::DirectoryChanged {
            after: observed, ..
        }
        | Event::DirectoryUndoIntent {
            before: observed, ..
        }
        | Event::DirectoryRestored {
            after: observed, ..
        } => put_observation(&mut out, observed),
        Event::OldPreserved {
            observed, scope, ..
        }
        | Event::Published {
            observed, scope, ..
        } => {
            put_observation(&mut out, observed);
            out.extend_from_slice(&scope);
        }
        Event::UndoIntent { current, .. } => optional(&mut out, current),
        Event::UndoneEffect {
            restored, scope, ..
        } => {
            optional(&mut out, restored);
            out.extend_from_slice(&scope);
        }
        _ => {}
    }
    out
}
pub fn decode_event(bytes: &[u8]) -> io::Result<Event> {
    if bytes.is_empty() || bytes.len() > MAX_EVENT {
        return Err(bad("Evento inválido"));
    }
    let mut r = Reader { data: bytes, at: 1 };
    let tag = bytes[0];
    let index = if matches!(tag,1..=5|7..=17|20..=25|27..=33) {
        r.u32().map_err(bad)?
    } else {
        0
    };
    let e = match tag {
        34 => Event::PlacementRegistered {
            seal: r.array().map_err(bad)?,
        },
        32 => Event::WorkspaceCreateIntent {
            transaction: index,
            seal: r.array().map_err(bad)?,
        },
        33 => Event::WorkspaceMaterialized(index),
        30 => Event::StageMetadataIntent {
            node: index,
            before: observation(&mut r)?,
            seal: r.array().map_err(bad)?,
        },
        31 => Event::StageMetadataApplied {
            node: index,
            after: observation(&mut r)?,
        },
        28 => Event::NodeCreateIntent {
            node: index,
            seal: r.array().map_err(bad)?,
        },
        29 => Event::NodeMaterialized(index),
        0 => Event::Approval {
            ticket: r.array().map_err(bad)?,
        },
        1 => Event::WorkspaceIntent(index),
        2 => Event::WorkspaceCreated {
            transaction: index,
            object: object(&mut r)?,
        },
        3 => Event::NodeCreated {
            node: index,
            object: object(&mut r)?,
        },
        4 => Event::NodeVerified {
            node: index,
            observed: observation(&mut r)?,
        },
        5 => Event::WorkspaceSynced(index),
        6 => Event::StagingComplete,
        7 => Event::EffectIntent(index),
        8 => Event::AbsenceChecked(index),
        9 => Event::OldPreserved {
            transaction: index,
            observed: observation(&mut r)?,
            scope: r.array().map_err(bad)?,
        },
        10 => Event::BackupSynced(index),
        11 => Event::PublishIntent(index),
        12 => Event::Published {
            transaction: index,
            observed: observation(&mut r)?,
            scope: r.array().map_err(bad)?,
        },
        13 => Event::EffectSynced(index),
        14 => Event::Committed(index),
        15 => Event::DirectoryIntent {
            index,
            before: observation(&mut r)?,
        },
        16 => Event::DirectoryChanged {
            index,
            after: observation(&mut r)?,
        },
        17 => Event::DirectorySynced(index),
        18 => Event::Completed,
        19 => Event::UndoBegin,
        20 => Event::UndoIntent {
            transaction: index,
            current: optional_read(&mut r)?,
        },
        21 => Event::UndoneEffect {
            transaction: index,
            restored: optional_read(&mut r)?,
            scope: r.array().map_err(bad)?,
        },
        22 => Event::UndoSynced(index),
        23 => Event::DirectoryUndoIntent {
            index,
            before: observation(&mut r)?,
        },
        24 => Event::DirectoryRestored {
            index,
            after: observation(&mut r)?,
        },
        25 => Event::DirectoryUndoSynced(index),
        26 => Event::Undone,
        27 => Event::NamespaceIntent {
            transaction: index,
            action: match r.take(1).map_err(bad)?[0] {
                0 => NamespaceAction::Preserve,
                1 => NamespaceAction::Publish,
                _ => return Err(bad("Acción namespace inválida")),
            },
            seal: r.array().map_err(bad)?,
        },
        _ => return Err(bad("Versión/tag de evento no soportado")),
    };
    if r.at != bytes.len() {
        return Err(bad("Evento con bytes sobrantes"));
    }
    Ok(e)
}
pub(crate) fn header(s: &Schema) -> Vec<u8> {
    let mut b = Vec::with_capacity(HEADER);
    b.extend_from_slice(MAGIC);
    b.extend_from_slice(&s.proof.encode());
    let mut h = Sha256::new();
    h.update(b"Files:sync-effect-journal:v1\0");
    h.update(&b);
    b.extend_from_slice(&h.finalize());
    b
}
pub(crate) fn frame(sequence: u64, previous: [u8; 32], event: Event) -> io::Result<Vec<u8>> {
    let body = encode_event(event);
    if sequence == 0 || sequence > MAX_EVENTS || body.len() > MAX_EVENT {
        return Err(bad("Journal agotado"));
    }
    let mut b = Vec::with_capacity(76 + body.len());
    b.extend_from_slice(&(body.len() as u32).to_le_bytes());
    b.extend_from_slice(&sequence.to_le_bytes());
    b.extend_from_slice(&previous);
    b.extend_from_slice(&body);
    let mut h = Sha256::new();
    h.update(b"Files:sync-effect-record:v1\0");
    h.update(&b);
    b.extend_from_slice(&h.finalize());
    Ok(b)
}
pub struct Replay {
    pub state: State,
    pub sequence: u64,
    pub length: u64,
    pub tail: [u8; 32],
}
/// Strict streaming history: a torn/corrupt tail is never silently truncated or
/// converted into a successful committed state. No filesystem or approval proof.
pub fn replay(s: &Schema, reader: &mut impl io::Read) -> io::Result<Replay> {
    let expected = header(s);
    let mut initial = vec![0; HEADER];
    reader.read_exact(&mut initial)?;
    if initial != expected {
        return Err(bad("Identidad/cabecera de journal no coincide"));
    }
    let mut previous: [u8; 32] = initial[HEADER - 32..]
        .try_into()
        .map_err(|_| bad("Sello inválido"))?;
    let mut sequence = 0;
    let mut length = HEADER as u64;
    let mut state = State::new(s);
    loop {
        let mut size = [0; 4];
        let mut first = [0; 1];
        if reader.read(&mut first)? == 0 {
            break;
        }
        size[0] = first[0];
        reader.read_exact(&mut size[1..])?;
        let n = u32::from_le_bytes(size) as usize;
        if n == 0 || n > MAX_EVENT || sequence >= MAX_EVENTS {
            return Err(bad("Evento/cola excesivo"));
        }
        let mut rest = vec![0; 40 + n + 32];
        reader.read_exact(&mut rest)?;
        let next = u64::from_le_bytes(
            rest[..8]
                .try_into()
                .map_err(|_| bad("Secuencia inválida"))?,
        );
        let mut h = Sha256::new();
        h.update(b"Files:sync-effect-record:v1\0");
        h.update(size);
        h.update(&rest[..40 + n]);
        let digest: [u8; 32] = h.finalize().into();
        if next != sequence + 1 || rest[8..40] != previous || rest[40 + n..] != digest {
            return Err(bad("Journal corrupto/fuera de secuencia"));
        }
        state.accept(s, decode_event(&rest[40..40 + n])?)?;
        length += 76 + n as u64;
        if length > MAX_JOURNAL {
            return Err(bad("Journal excede presupuesto"));
        }
        sequence = next;
        previous = digest;
    }
    Ok(Replay {
        state,
        sequence,
        length,
        tail: previous,
    })
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::{
        directory_sync::{Mode, Side},
        directory_sync_manifest::capture,
        directory_sync_plan::{Decision, Options, Plan},
        directory_sync_wire::Intent,
    };
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
        sync::atomic::{AtomicBool, AtomicU64, Ordering},
    };
    fn schema(mode: Mode) -> Schema {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = std::env::temp_dir().join(format!(
            "files-journal-schema-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let a = base.join("a");
        let b = base.join("b");
        let saved = base.join("saved");
        for d in [&a, &b, &saved] {
            fs::create_dir_all(d).unwrap();
        }
        fs::set_permissions(&saved, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&a, fs::Permissions::from_mode(0o750)).unwrap();
        for d in [a.join("shared"), b.join("shared")] {
            fs::create_dir(&d).unwrap();
        }
        fs::set_permissions(a.join("shared"), fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(b.join("shared"), fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(a.join("shared/new-child"), b"child").unwrap();
        fs::write(a.join("new"), b"new").unwrap();
        fs::hard_link(a.join("new"), a.join("linked")).unwrap();
        fs::write(a.join("replace"), b"long replacement").unwrap();
        fs::write(b.join("replace"), b"old").unwrap();
        fs::write(b.join("retire"), b"retired").unwrap();
        fs::create_dir(a.join("tree")).unwrap();
        fs::write(a.join("tree/leaf"), b"leaf").unwrap();
        symlink("new", a.join("symlink")).unwrap();
        let cancel = AtomicBool::new(false);
        let prepared = capture(
            Intent {
                version: 1,
                id: 9,
                left: a,
                right: b,
                expected: None,
                mode,
                read_limit: 1024 * 1024,
            },
            &cancel,
            |_| Ok(()),
        )
        .unwrap();
        let record = prepared.record().unwrap();
        let mut p = Plan::new(
            &record,
            Options {
                root_metadata: true,
                ..Options::default()
            },
        )
        .unwrap();
        for i in 0..p.lines().len() {
            if p.lines()[i].decision() == Decision::Pending {
                let receipt = p.receipt(i).unwrap();
                if p.choose(receipt, Decision::Retire).is_err() {
                    let receipt = p.receipt(i).unwrap();
                    p.choose(
                        receipt,
                        Decision::Take(match mode {
                            Mode::Update(s) | Mode::Mirror(s) => s,
                            Mode::Merge => Side::Left,
                        }),
                    )
                    .unwrap();
                }
            }
        }
        let physical = Physical::derive(&p).unwrap();
        let store = Store::create(
            &saved,
            &physical,
            &prepared,
            [9; 16],
            crate::directory_sync_metadata::DEFAULT_STREAM_BYTES,
            &cancel,
        )
        .unwrap();
        let s = Schema::derive(&store, &physical).unwrap();
        drop(store);
        fs::remove_dir_all(base).unwrap();
        s
    }
    fn observation(
        object: Object,
        meta: Meta,
        size: u64,
        links: u64,
        content: [u8; 32],
    ) -> Observation {
        Observation {
            object,
            revision: Revision {
                size,
                mode: meta.mode,
                uid: meta.uid,
                gid: meta.gid,
                links,
                mtime_seconds: meta.modified.seconds,
                mtime_nanos: meta.modified.nanos,
                ctime_seconds: 1,
                ctime_nanos: 0,
            },
            content,
            metadata: meta.digest(meta.modified),
        }
    }
    fn stage(s: &Schema) -> (State, Vec<Event>) {
        let mut state = State::new(s);
        let mut events = Vec::new();
        let mut push = |e| {
            state.accept(s, e).unwrap();
            events.push(e);
        };
        push(Event::Approval { ticket: [1; 16] });
        for (i, t) in s.transactions.iter().enumerate() {
            push(Event::WorkspaceIntent(i as u32));
            push(Event::WorkspaceCreated {
                transaction: i as u32,
                object: Object {
                    identity: Identity {
                        device: t.device,
                        inode: 1_000_000 + i as u64,
                        kind: Kind::Directory,
                    },
                    mount: t.mount,
                },
            });
        }
        let objects: Vec<_> = s
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| Object {
                identity: Identity {
                    inode: 2_000_000 + n.group.map_or(i as u64, |g| 100_000 + g as u64),
                    ..n.object.identity
                },
                mount: n.object.mount,
            })
            .collect();
        for (i, &object) in objects.iter().enumerate() {
            push(Event::NodeCreated {
                node: i as u32,
                object,
            });
        }
        for (i, n) in s.nodes.iter().enumerate() {
            push(Event::NodeVerified {
                node: i as u32,
                observed: observation(
                    objects[i],
                    n.meta,
                    n.size.unwrap_or(4096),
                    n.links.unwrap_or(2),
                    n.content,
                ),
            });
        }
        for i in 0..s.transactions.len() {
            push(Event::WorkspaceSynced(i as u32));
        }
        push(Event::StagingComplete);
        (state, events)
    }
    fn finish(s: &Schema, state: &mut State, events: &mut Vec<Event>) {
        for (i, t) in s.transactions.iter().enumerate() {
            let i = i as u32;
            let mut batch = vec![Event::EffectIntent(i)];
            if let Some(old) = t.old {
                batch.push(Event::OldPreserved {
                    transaction: i,
                    observed: old,
                    scope: t.old_scope,
                });
                batch.push(Event::BackupSynced(i));
            } else {
                batch.push(Event::AbsenceChecked(i));
            }
            for e in batch {
                state.accept(s, e).unwrap();
                events.push(e);
            }
            if t.operation != Operation::Retire {
                let e = Event::PublishIntent(i);
                state.accept(s, e).unwrap();
                events.push(e);
                let e = Event::Published {
                    transaction: i,
                    observed: state.verified[t.nodes.start],
                    scope: state.staged_scope(s, i as usize).unwrap(),
                };
                state.accept(s, e).unwrap();
                events.push(e);
            }
            for e in [Event::EffectSynced(i), Event::Committed(i)] {
                state.accept(s, e).unwrap();
                events.push(e);
            }
        }
        for (i, d) in s.directories.iter().enumerate() {
            let before = observation(d.object, d.old, 4096, 2, d.original_content);
            let mut after = observation(d.object, d.new, 4096, 2, d.original_content);
            if d.after_children {
                after.revision.mtime_seconds = before.revision.mtime_seconds;
                after.revision.mtime_nanos = before.revision.mtime_nanos;
                after.metadata = d.new.digest(mtime(before));
            }
            for e in [
                Event::DirectoryIntent {
                    index: i as u32,
                    before,
                },
                Event::DirectoryChanged {
                    index: i as u32,
                    after,
                },
                Event::DirectorySynced(i as u32),
            ] {
                state.accept(s, e).unwrap();
                events.push(e);
            }
        }
        state.accept(s, Event::Completed).unwrap();
        events.push(Event::Completed);
    }
    #[test]
    fn all_modes_stage_publish_commit_undo_and_stream_roundtrip() {
        for mode in [
            Mode::Merge,
            Mode::Update(Side::Left),
            Mode::Update(Side::Right),
            Mode::Mirror(Side::Left),
            Mode::Mirror(Side::Right),
        ] {
            let s = schema(mode);
            assert!(!s.directories.is_empty());
            let (mut state, mut events) = stage(&s);
            finish(&s, &mut state, &mut events);
            assert_eq!(state.phase(), Phase::Completed);
            state.accept(&s, Event::UndoBegin).unwrap();
            events.push(Event::UndoBegin);
            for i in (0..s.transactions.len()).rev() {
                let t = &s.transactions[i];
                for e in [
                    Event::UndoIntent {
                        transaction: i as u32,
                        current: state.published[i],
                    },
                    Event::UndoneEffect {
                        transaction: i as u32,
                        restored: t.old,
                        scope: if t.old.is_some() {
                            t.old_scope
                        } else {
                            [0; 32]
                        },
                    },
                    Event::UndoSynced(i as u32),
                ] {
                    state.accept(&s, e).unwrap();
                    events.push(e);
                }
            }
            for (i, d) in s.directories.iter().enumerate() {
                let before = observation(d.object, d.new, 4096, 2, d.original_content);
                let after = observation(d.object, d.old, 4096, 2, d.original_content);
                for e in [
                    Event::DirectoryUndoIntent {
                        index: i as u32,
                        before,
                    },
                    Event::DirectoryRestored {
                        index: i as u32,
                        after,
                    },
                    Event::DirectoryUndoSynced(i as u32),
                ] {
                    state.accept(&s, e).unwrap();
                    events.push(e);
                }
            }
            state.accept(&s, Event::Undone).unwrap();
            events.push(Event::Undone);
            assert!(state.accept(&s, Event::Completed).is_err());
            let mut bytes = header(&s);
            let mut previous = bytes[HEADER - 32..].try_into().unwrap();
            for (i, e) in events.iter().enumerate() {
                let body = encode_event(*e);
                assert!(body.len() <= MAX_EVENT);
                assert_eq!(decode_event(&body).unwrap(), *e);
                let f = frame(i as u64 + 1, previous, *e).unwrap();
                previous = f[f.len() - 32..].try_into().unwrap();
                bytes.extend(f);
            }
            let parsed = replay(&s, &mut bytes.as_slice()).unwrap();
            assert_eq!(parsed.state.phase(), Phase::Undone);
            assert_eq!(parsed.sequence, events.len() as u64);
            assert_eq!(parsed.length, bytes.len() as u64);
        }
    }
    #[test]
    fn ordering_identity_and_cohort_claims_fail_closed() {
        let s = schema(Mode::Merge);
        let (mut state, events) = stage(&s);
        let mut prefix = State::new(&s);
        for e in events {
            assert!(prefix.accept(&s, Event::EffectIntent(0)).is_err());
            prefix.accept(&s, e).unwrap();
        }
        assert!(state.accept(&s, Event::Committed(0)).is_err());
        state.accept(&s, Event::EffectIntent(0)).unwrap();
        assert!(state.accept(&s, Event::UndoBegin).is_err());
        assert!(state.accept(&s, Event::EffectSynced(0)).is_err());
        let mut a = State::new(&s);
        a.accept(&s, Event::Approval { ticket: [1; 16] }).unwrap();
        a.accept(&s, Event::WorkspaceIntent(0)).unwrap();
        let t = &s.transactions[0];
        let mut object = Object {
            identity: Identity {
                device: t.device,
                inode: 1,
                kind: Kind::Directory,
            },
            mount: t.mount,
        };
        let &(dev, ino) = s.protected.iter().next().unwrap();
        object.identity.device = dev;
        object.identity.inode = ino;
        assert!(
            a.accept(
                &s,
                Event::WorkspaceCreated {
                    transaction: 0,
                    object
                }
            )
            .is_err()
        );
        let mut a = State::new(&s);
        let (_, events) = stage(&s);
        for e in events {
            if let Event::NodeCreated { node, mut object } = e
                && let Some(group) = s.nodes[node as usize].group
                && a.groups.contains_key(&group)
            {
                object.identity.inode += 1;
                assert!(a.accept(&s, Event::NodeCreated { node, object }).is_err());
            }
            if let Event::NodeVerified { node, mut observed } = e {
                observed.revision.uid ^= 1;
                assert!(
                    a.accept(&s, Event::NodeVerified { node, observed })
                        .is_err()
                );
            }
            a.accept(&s, e).unwrap();
        }
    }
    #[test]
    fn codec_rejects_torn_corrupt_reordered_and_foreign_history() {
        let s = schema(Mode::Merge);
        let first = Event::Approval { ticket: [1; 16] };
        let h = header(&s);
        let tail = h[HEADER - 32..].try_into().unwrap();
        let f = frame(1, tail, first).unwrap();
        let mut bytes = h.clone();
        bytes.extend(&f);
        for cut in 1..f.len() {
            assert!(replay(&s, &mut &bytes[..HEADER + cut]).is_err());
        }
        let mut corrupt = bytes.clone();
        *corrupt.last_mut().unwrap() ^= 1;
        assert!(replay(&s, &mut corrupt.as_slice()).is_err());
        let mut gap = h.clone();
        gap.extend(frame(2, tail, first).unwrap());
        assert!(replay(&s, &mut gap.as_slice()).is_err());
        let mut wrong = h.clone();
        wrong[10] ^= 1;
        assert!(replay(&s, &mut wrong.as_slice()).is_err());
        let mut illegal = h;
        illegal.extend(frame(1, tail, Event::Completed).unwrap());
        assert!(replay(&s, &mut illegal.as_slice()).is_err());
        assert!(decode_event(&[255]).is_err());
        let mut extra = encode_event(first);
        extra.push(0);
        assert!(decode_event(&extra).is_err());
        assert!(frame(MAX_EVENTS + 1, tail, first).is_err());
    }
}

#[cfg(target_os = "linux")]
mod namespace;

#[cfg(target_os = "linux")]
mod staging;

#[cfg(target_os = "linux")]
pub(crate) mod create;

#[cfg(target_os = "linux")]
pub(crate) mod stage_meta;

#[cfg(target_os = "linux")]
pub(crate) mod workspace;
