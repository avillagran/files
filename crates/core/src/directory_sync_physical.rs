//! Derived, bounded physical sync blueprint. No syscalls, approval or executor.
//! All sources/metadata must be staged before any destination is preserved or
//! published. Cohort metadata constraints must be resolved before approval.
use crate::{
    directory_sync::Side,
    directory_sync_manifest::Location,
    directory_sync_plan::{Effect, Hardlinks, Metadata, Plan},
    directory_sync_record::{EntryView, View},
    move_manifest::Kind,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    ops::Range,
};
pub const MAX_NODES: usize = 2 * crate::directory_compare::MAX_ITEMS;
pub const MAX_TRANSACTIONS: usize = MAX_NODES;
pub const MAX_STAGE_PAYLOAD: u64 = crate::directory_compare::MAX_READ;
// Fixed numeric references, no duplicated path/attribute payloads.
pub const MAX_WIRE: usize = 128 + MAX_NODES * 192;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recipe {
    /// Reacquire the complete captured values, not just their digest.
    Full(Location),
    /// Keep base owner/access/attributes; overlay source permission bits and,
    /// for non-directories, mtime. Directory mtime follows own child effects.
    Basic {
        base: Location,
        source: Location,
        mtime: bool,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Add,
    Replace,
    Retire,
    IsolatedMetadata,
}
#[derive(Debug)]
pub struct Transaction {
    row: usize,
    side: usize,
    parent: Location,
    old: Option<Location>,
    old_scope: Range<usize>,
    mount: u64,
    operation: Operation,
    nodes: Range<usize>,
}
impl Transaction {
    pub fn row(&self) -> usize {
        self.row
    }
    pub fn side(&self) -> usize {
        self.side
    }
    pub fn parent(&self) -> Location {
        self.parent
    }
    pub fn old(&self) -> Option<Location> {
        self.old
    }
    pub fn old_items(&self) -> usize {
        self.old_scope.len()
    }
    pub fn backup_entries(&self) -> Range<usize> {
        self.old_scope.clone()
    }
    pub fn mount(&self) -> u64 {
        self.mount
    }
    pub fn operation(&self) -> Operation {
        self.operation
    }
    pub fn nodes(&self) -> Range<usize> {
        self.nodes.clone()
    }
}
#[derive(Debug)]
pub struct Node {
    transaction: usize,
    source: Location,
    parent_node: Option<usize>,
    recipe: Recipe,
    cohort: Option<usize>,
}
impl Node {
    pub fn transaction(&self) -> usize {
        self.transaction
    }
    pub fn source(&self) -> Location {
        self.source
    }
    pub fn parent_node(&self) -> Option<usize> {
        self.parent_node
    }
    pub fn recipe(&self) -> Recipe {
        self.recipe
    }
    pub fn cohort(&self) -> Option<usize> {
        self.cohort
    }
}
#[derive(Debug)]
pub struct Cohort {
    destination: usize,
    mount: u64,
    identity: (u64, u64),
    members: Vec<usize>,
    source_names: usize,
    external_links: u64,
    // Different recipes can still yield identical values. Never silently split
    // or choose one member's attributes; acquire values and compare them first.
    needs_metadata_values: bool,
}
impl Cohort {
    pub fn destination(&self) -> usize {
        self.destination
    }
    pub fn mount(&self) -> u64 {
        self.mount
    }
    pub fn identity(&self) -> (u64, u64) {
        self.identity
    }
    pub fn members(&self) -> &[usize] {
        &self.members
    }
    pub fn source_names(&self) -> usize {
        self.source_names
    }
    pub fn omitted_sources(&self) -> usize {
        self.source_names - self.members.len()
    }
    pub fn external_links(&self) -> u64 {
        self.external_links
    }
    pub fn needs_metadata_values(&self) -> bool {
        self.needs_metadata_values
    }
}
#[derive(Debug)]
pub struct DirectoryMetadata {
    row: usize,
    target: Location,
    recipe: Recipe,
}
impl DirectoryMetadata {
    pub fn row(&self) -> usize {
        self.row
    }
    pub fn target(&self) -> Location {
        self.target
    }
    pub fn recipe(&self) -> Recipe {
        self.recipe
    }
}
/// Names sharing an old leaf inode whose ctime can change on backup rename.
/// Outside names are never opened or changed by content/attribute application;
/// the kernel's inode ctime effect of renaming a link is still observable there.
#[derive(Debug)]
pub struct AliasImpact {
    identity: (u64, u64),
    captured: Vec<Location>,
    external: u64,
}
impl AliasImpact {
    pub fn identity(&self) -> (u64, u64) {
        self.identity
    }
    pub fn captured(&self) -> &[Location] {
        &self.captured
    }
    pub fn external(&self) -> u64 {
        self.external
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageStep {
    Directory {
        node: usize,
    },
    File {
        node: usize,
    },
    Symlink {
        node: usize,
    },
    /// Link the already-created staged inode, without following a symlink.
    Link {
        node: usize,
        leader: usize,
    },
}
pub struct Physical<'p, 'r> {
    plan: &'p Plan<'r>,
    max_stage_payload: u64,
    stage_payload: u64,
    transactions: Vec<Transaction>,
    nodes: Vec<Node>,
    preserved_entries: Vec<Location>,
    cohorts: Vec<Cohort>,
    directory_metadata: Vec<DirectoryMetadata>,
    dirty_parents: Vec<Location>,
    aliases: Vec<AliasImpact>,
}
fn bad(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn side(s: Side) -> usize {
    match s {
        Side::Left => 0,
        Side::Right => 1,
    }
}
fn parent(path: &[u8]) -> &[u8] {
    path.iter()
        .rposition(|&c| c == b'/')
        .map_or(b"", |i| &path[..i])
}
fn locate(view: &View<'_>, s: usize, path: &[u8]) -> io::Result<Location> {
    let index = view.roots[s]
        .entries
        .binary_search_by_key(&path, |e| e.relative)
        .map_err(|_| bad("Padre físico ausente"))?;
    Ok(Location { side: s, index })
}
fn entry<'a, 'r>(view: &'a View<'r>, l: Location) -> &'a EntryView<'r> {
    &view.roots[l.side].entries[l.index]
}
// Root plus its slash-descendants; raw-order siblings such as a- are excluded.
fn scope(view: &View<'_>, l: Location) -> Vec<usize> {
    let entries = &view.roots[l.side].entries;
    let root = &entries[l.index];
    let mut prefix = root.relative.to_vec();
    prefix.push(b'/');
    let start = entries.partition_point(|e| e.relative < prefix.as_slice());
    let mut out = vec![l.index];
    if root.identity.kind == Kind::Directory {
        out.extend(
            (start..entries.len()).take_while(|&i| entries[i].relative.starts_with(&prefix)),
        );
    }
    out
}
fn recipe(policy: Metadata, view: &View<'_>, source: Location, old: Option<Location>) -> Recipe {
    let Some(base) =
        old.filter(|&l| entry(view, l).identity.kind == entry(view, source).identity.kind)
    else {
        return Recipe::Full(source);
    };
    match policy {
        Metadata::Ignore => Recipe::Full(base),
        Metadata::Basic => Recipe::Basic {
            base,
            source,
            mtime: entry(view, source).identity.kind != Kind::Directory,
        },
        Metadata::ExactObserved => Recipe::Full(source),
    }
}
fn same_recipe(view: &View<'_>, a: Recipe, b: Recipe) -> bool {
    match (a, b) {
        (Recipe::Full(a), Recipe::Full(b)) => entry(view, a).metadata == entry(view, b).metadata,
        (
            Recipe::Basic {
                base: a,
                source: x,
                mtime: am,
            },
            Recipe::Basic {
                base: b,
                source: y,
                mtime: bm,
            },
        ) => {
            let (a, b, x, y) = (
                entry(view, a),
                entry(view, b),
                entry(view, x),
                entry(view, y),
            );
            a.metadata == b.metadata
                && am == bm
                && (x.revision.mode & 0o7777) == (y.revision.mode & 0o7777)
                && (!am
                    || (x.revision.mtime_seconds, x.revision.mtime_nanos)
                        == (y.revision.mtime_seconds, y.revision.mtime_nanos))
        }
        _ => false,
    }
}
impl<'p, 'r> Physical<'p, 'r> {
    pub fn derive(plan: &'p Plan<'r>) -> io::Result<Self> {
        Self::derive_with_limit(plan, MAX_STAGE_PAYLOAD)
    }
    /// Separate from hashing/read budget: IndependentNames can multiply payload.
    /// Includes literal symlink target bytes, not attribute/storage allocations.
    pub fn derive_with_limit(plan: &'p Plan<'r>, max_stage_payload: u64) -> io::Result<Self> {
        if max_stage_payload > MAX_STAGE_PAYLOAD {
            return Err(bad("Presupuesto de preparación inválido"));
        }
        if plan.unresolved() != 0 {
            return Err(bad("Hay conflictos pendientes"));
        }
        let view = plan.record().view()?;
        let mut transactions = Vec::new();
        let mut nodes = Vec::new();
        let mut preserved_entries = Vec::new();
        let mut directory_metadata = Vec::new();
        let mut dirty = BTreeSet::new();
        let mut old_leaves = BTreeSet::new();
        for row in plan.effect_order()? {
            let effect = plan.effect(row)?;
            let locations = plan.lines()[row].locations();
            let (source, destination, operation) = match effect {
                Effect::CopyFrom(s) => (Some(side(s)), 1 - side(s), Operation::Add),
                Effect::PreserveAndReplaceFrom(s) => {
                    (Some(side(s)), 1 - side(s), Operation::Replace)
                }
                Effect::PreserveAndSetMetadataFrom(s) => {
                    (Some(side(s)), 1 - side(s), Operation::IsolatedMetadata)
                }
                Effect::PreserveAndRetire => {
                    let destination = if locations[0].is_some() { 0 } else { 1 };
                    (None, destination, Operation::Retire)
                }
                _ => return Err(bad("Efecto lógico inesperado")),
            };
            let old = locations[destination].map(|index| Location {
                side: destination,
                index,
            });
            let source = source
                .map(|s| {
                    locations[s]
                        .map(|index| Location { side: s, index })
                        .ok_or_else(|| bad("Fuente ausente"))
                })
                .transpose()?;
            if operation == Operation::IsolatedMetadata && plan.lines()[row].shared_directory() {
                let target = old.ok_or_else(|| bad("Directorio destino ausente"))?;
                directory_metadata.push(DirectoryMetadata {
                    row,
                    target,
                    recipe: recipe(
                        plan.options().metadata,
                        &view,
                        source.ok_or_else(|| bad("Fuente ausente"))?,
                        old,
                    ),
                });
                continue;
            }
            let path = plan.name(row).ok_or_else(|| bad("Fila ausente"))?;
            if path.is_empty() {
                return Err(bad("La raíz no puede reemplazarse ni retirarse"));
            }
            let target_parent = locate(&view, destination, parent(path))?;
            let mount = entry(&view, target_parent).mount;
            let old_scope = old.map(|l| scope(&view, l)).unwrap_or_default();
            if let Some(old) = old {
                if old_scope
                    .iter()
                    .any(|&index| view.roots[destination].entries[index].mount != mount)
                {
                    return Err(bad(
                        "El destino cubre un montaje; retiro recuperable no soportado",
                    ));
                }
                let e = entry(&view, old);
                if e.identity.kind != Kind::Directory {
                    old_leaves.insert((e.identity.device, e.identity.inode));
                }
            }
            let backup_start = preserved_entries.len();
            preserved_entries.extend(old_scope.iter().map(|&index| Location {
                side: destination,
                index,
            }));
            if preserved_entries.len() > MAX_NODES {
                return Err(bad("Preservación excede límite"));
            }
            let start = nodes.len();
            let transaction = transactions.len();
            if let Some(source) = source {
                let mut parent_nodes: BTreeMap<&[u8], usize> = BTreeMap::new();
                for index in scope(&view, source) {
                    let loc = Location {
                        side: source.side,
                        index,
                    };
                    let e = entry(&view, loc);
                    let parent_node = if index == source.index {
                        None
                    } else {
                        Some(
                            *parent_nodes
                                .get(parent(e.relative))
                                .ok_or_else(|| bad("Padre de stage ausente"))?,
                        )
                    };
                    let previous = if index == source.index { old } else { None };
                    let n = nodes.len();
                    nodes.push(Node {
                        transaction,
                        source: loc,
                        parent_node,
                        recipe: recipe(plan.options().metadata, &view, loc, previous),
                        cohort: None,
                    });
                    if e.identity.kind == Kind::Directory {
                        parent_nodes.insert(e.relative, n);
                    }
                }
            }
            if nodes.len() > MAX_NODES || transactions.len() >= MAX_TRANSACTIONS {
                return Err(bad("Plan físico excede límite"));
            }
            dirty.insert((target_parent.side, target_parent.index));
            transactions.push(Transaction {
                row,
                side: destination,
                parent: target_parent,
                old,
                old_scope: backup_start..preserved_entries.len(),
                mount,
                operation,
                nodes: start..nodes.len(),
            });
        }
        let mut counts: BTreeMap<(u64, u64), [usize; 2]> = BTreeMap::new();
        for (s, root) in view.roots.iter().enumerate() {
            for e in &root.entries {
                if e.identity.kind != Kind::Directory {
                    counts
                        .entry((e.identity.device, e.identity.inode))
                        .or_insert([0; 2])[s] += 1;
                }
            }
        }
        let mut cohorts: Vec<Cohort> = Vec::new();
        let mut groups = BTreeMap::new();
        if plan.options().hardlinks == Hardlinks::PreserveInternal {
            for i in 0..nodes.len() {
                let node = &nodes[i];
                let e = entry(&view, node.source);
                if e.identity.kind == Kind::Directory {
                    continue;
                }
                let t = &transactions[node.transaction];
                let key = (t.side, e.identity.device, e.identity.inode);
                let group = if let Some(&g) = groups.get(&key) {
                    let c: &mut Cohort = &mut cohorts[g];
                    if c.mount != t.mount {
                        return Err(bad("Grupo hardlink seleccionado cruza montajes destino"));
                    }
                    let first = nodes[c.members[0]].recipe;
                    c.needs_metadata_values |= !same_recipe(&view, first, node.recipe);
                    c.members.push(i);
                    g
                } else {
                    let g = cohorts.len();
                    groups.insert(key, g);
                    cohorts.push(Cohort {
                        destination: t.side,
                        mount: t.mount,
                        identity: (e.identity.device, e.identity.inode),
                        members: vec![i],
                        source_names: counts[&(e.identity.device, e.identity.inode)]
                            [node.source.side],
                        external_links: e.revision.links
                            - counts[&(e.identity.device, e.identity.inode)]
                                .iter()
                                .sum::<usize>() as u64,
                        needs_metadata_values: false,
                    });
                    g
                };
                nodes[i].cohort = Some(group);
            }
        }
        let mut stage_payload = 0u64;
        for (i, n) in nodes.iter().enumerate() {
            let e = entry(&view, n.source);
            if e.identity.kind == Kind::Directory
                || n.cohort.is_some_and(|g| cohorts[g].members[0] != i)
            {
                continue;
            }
            stage_payload = stage_payload
                .checked_add(e.revision.size)
                .filter(|&v| v <= max_stage_payload)
                .ok_or_else(|| bad("Payload de preparación excede presupuesto"))?;
        }
        let mut aliases = Vec::new();
        for group in view.links()? {
            if old_leaves.contains(&(group.device, group.inode)) {
                aliases.push(AliasImpact {
                    identity: (group.device, group.inode),
                    captured: group.occurrences,
                    external: group.external_links,
                });
            }
        }
        Ok(Self {
            plan,
            max_stage_payload,
            stage_payload,
            transactions,
            nodes,
            preserved_entries,
            cohorts,
            directory_metadata,
            dirty_parents: dirty
                .into_iter()
                .map(|(side, index)| Location { side, index })
                .collect(),
            aliases,
        })
    }
    pub fn stage_payload(&self) -> u64 {
        self.stage_payload
    }
    pub fn max_stage_payload(&self) -> u64 {
        self.max_stage_payload
    }
    pub fn plan(&self) -> &Plan<'r> {
        self.plan
    }
    pub fn transactions(&self) -> &[Transaction] {
        &self.transactions
    }
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }
    pub fn preserved_entries(&self) -> &[Location] {
        &self.preserved_entries
    }
    pub fn cohorts(&self) -> &[Cohort] {
        &self.cohorts
    }
    pub fn directory_metadata(&self) -> &[DirectoryMetadata] {
        &self.directory_metadata
    }
    pub fn dirty_parents(&self) -> &[Location] {
        &self.dirty_parents
    }
    pub fn aliases(&self) -> &[AliasImpact] {
        &self.aliases
    }
    /// This is only the count of cohort-equality constraints, not a readiness
    /// predicate. Every recipe still needs actual values, proofs and staging.
    pub fn metadata_constraints(&self) -> usize {
        self.cohorts
            .iter()
            .filter(|g| g.needs_metadata_values)
            .count()
    }
    /// All these creation steps precede any backup/publication. Metadata values
    /// and cohort equality must be resolved first. Later members link to their
    /// staged leader, never to an old destination or live source inode.
    pub fn staging_order(&self) -> io::Result<Vec<StageStep>> {
        let view = self.plan.record().view()?;
        Ok(self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| {
                if let Some(group) = n.cohort {
                    let leader = self.cohorts[group].members[0];
                    if leader != i {
                        return StageStep::Link { node: i, leader };
                    }
                }
                match entry(&view, n.source).identity.kind {
                    Kind::Directory => StageStep::Directory { node: i },
                    Kind::File => StageStep::File { node: i },
                    Kind::Symlink => StageStep::Symlink { node: i },
                }
            })
            .collect())
    }
    /// Final staging metadata: each non-directory inode once, then directory
    /// children before parents. All creation/cohort links must already exist.
    /// This order does not authorize applying metadata or publishing objects.
    pub fn stage_metadata_order(&self) -> io::Result<Vec<usize>> {
        let view = self.plan.record().view()?;
        let mut depths = Vec::with_capacity(self.nodes.len());
        let mut keys = Vec::with_capacity(self.nodes.len());
        for (i, node) in self.nodes.iter().enumerate() {
            let depth = if let Some(parent) = node.parent_node {
                depths
                    .get(parent)
                    .copied()
                    .ok_or_else(|| bad("Padre de stage fuera de orden"))?
                    + 1
            } else {
                0usize
            };
            depths.push(depth);
            if node.cohort.is_some_and(|g| self.cohorts[g].members[0] != i) {
                continue;
            }
            let directory = entry(&view, node.source).identity.kind == Kind::Directory;
            keys.push((directory, if directory { usize::MAX - depth } else { 0 }, i));
        }
        keys.sort_unstable();
        Ok(keys.into_iter().map(|(_, _, i)| i).collect())
    }
    /// Canonical numeric blueprint, rederived from evidence/choices. No decoder
    /// creates live state; acquired attribute values/stage identities are absent.
    pub fn encode(&self) -> io::Result<Vec<u8>> {
        fn u(out: &mut Vec<u8>, n: usize) {
            out.extend_from_slice(&(n as u32).to_le_bytes());
        }
        fn loc(out: &mut Vec<u8>, l: Location) {
            out.push(l.side as u8);
            u(out, l.index);
        }
        fn opt(out: &mut Vec<u8>, n: Option<usize>) {
            u(out, n.unwrap_or(u32::MAX as usize));
        }
        fn rec(out: &mut Vec<u8>, r: Recipe) {
            match r {
                Recipe::Full(l) => {
                    out.push(0);
                    loc(out, l);
                }
                Recipe::Basic {
                    base,
                    source,
                    mtime,
                } => {
                    out.push(1);
                    loc(out, base);
                    loc(out, source);
                    out.push(u8::from(mtime));
                }
            }
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"FLSPHY\0\x01");
        out.extend_from_slice(&self.plan.record().seal());
        out.extend_from_slice(&self.plan.seal()?);
        out.extend_from_slice(&self.max_stage_payload.to_le_bytes());
        out.extend_from_slice(&self.stage_payload.to_le_bytes());
        u(&mut out, self.transactions.len());
        for t in &self.transactions {
            u(&mut out, t.row);
            out.push(t.side as u8);
            loc(&mut out, t.parent);
            opt(&mut out, t.old.map(|l| l.index));
            u(&mut out, t.old_scope.start);
            u(&mut out, t.old_scope.end);
            out.extend_from_slice(&t.mount.to_le_bytes());
            out.push(match t.operation {
                Operation::Add => 0,
                Operation::Replace => 1,
                Operation::Retire => 2,
                Operation::IsolatedMetadata => 3,
            });
            u(&mut out, t.nodes.start);
            u(&mut out, t.nodes.end);
        }
        u(&mut out, self.nodes.len());
        for n in &self.nodes {
            u(&mut out, n.transaction);
            loc(&mut out, n.source);
            opt(&mut out, n.parent_node);
            rec(&mut out, n.recipe);
            opt(&mut out, n.cohort);
        }
        u(&mut out, self.cohorts.len());
        for c in &self.cohorts {
            out.push(c.destination as u8);
            out.extend_from_slice(&c.mount.to_le_bytes());
            out.extend_from_slice(&c.identity.0.to_le_bytes());
            out.extend_from_slice(&c.identity.1.to_le_bytes());
            u(&mut out, c.source_names);
            out.extend_from_slice(&c.external_links.to_le_bytes());
            out.push(u8::from(c.needs_metadata_values));
            u(&mut out, c.members.len());
            for &i in &c.members {
                u(&mut out, i);
            }
        }
        u(&mut out, self.directory_metadata.len());
        for d in &self.directory_metadata {
            u(&mut out, d.row);
            loc(&mut out, d.target);
            rec(&mut out, d.recipe);
        }
        u(&mut out, self.dirty_parents.len());
        for &p in &self.dirty_parents {
            loc(&mut out, p);
        }
        u(&mut out, self.preserved_entries.len());
        for &l in &self.preserved_entries {
            loc(&mut out, l);
        }
        u(&mut out, self.aliases.len());
        for a in &self.aliases {
            out.extend_from_slice(&a.identity.0.to_le_bytes());
            out.extend_from_slice(&a.identity.1.to_le_bytes());
            out.extend_from_slice(&a.external.to_le_bytes());
            u(&mut out, a.captured.len());
            for &l in &a.captured {
                loc(&mut out, l);
            }
        }
        if out.len() + 32 > MAX_WIRE {
            return Err(bad("Codificación física excede límite"));
        }
        let mut h = Sha256::new();
        h.update(b"Files:sync-physical-blueprint:v1\0");
        h.update(&out);
        out.extend_from_slice(&h.finalize());
        Ok(out)
    }
    pub fn seal(&self) -> io::Result<[u8; 32]> {
        let bytes = self.encode()?;
        bytes[bytes.len() - 32..]
            .try_into()
            .map_err(|_| bad("Sello ausente"))
    }
}
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::{
        directory_sync::{Mode, Side},
        directory_sync_manifest::{Prepared, capture},
        directory_sync_plan::{Decision, Options},
        directory_sync_record::{FIXED_ENTRY, Record},
        directory_sync_wire::Intent,
    };
    use std::{
        fs,
        os::unix::{
            ffi::OsStringExt,
            fs::{PermissionsExt, symlink},
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
                "files-sync-physical-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            let paths = [base.join("a"), base.join("b")];
            for p in &paths {
                fs::create_dir_all(p).unwrap();
            }
            Self { base, paths }
        }
        fn file(&self, s: usize, name: &str, bytes: &[u8]) {
            fs::write(self.paths[s].join(name), bytes).unwrap();
        }
        fn prepare(&self, mode: Mode) -> Prepared {
            capture(
                Intent {
                    version: 1,
                    id: 9,
                    left: self.paths[0].clone(),
                    right: self.paths[1].clone(),
                    expected: None,
                    mode,
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
    fn row(p: &Plan<'_>, name: &[u8]) -> usize {
        (0..p.lines().len())
            .find(|&i| p.name(i) == Some(name))
            .unwrap()
    }
    fn choose(p: &mut Plan<'_>, name: &[u8], d: Decision) {
        let i = row(p, name);
        p.choose(p.receipt(i).unwrap(), d).unwrap();
    }
    fn ignore() -> Options {
        Options {
            metadata: Metadata::Ignore,
            ..Options::default()
        }
    }
    #[test]
    fn staging_admission_bounds_replace_retire_and_cross_workspace_links() {
        let f = Fixture::new();
        f.file(0, "x", b"source");
        fs::hard_link(f.paths[0].join("x"), f.paths[0].join("y")).unwrap();
        f.file(1, "x", b"old");
        f.file(1, "z", b"retire");
        let prepared = f.prepare(Mode::Mirror(Side::Left));
        let record = prepared.record().unwrap();
        let mut p = Plan::new(&record, Options::default()).unwrap();
        choose(&mut p, b"x", Decision::Take(Side::Left));
        choose(&mut p, b"z", Decision::Retire);
        let physical = Physical::derive(&p).unwrap();
        let admission = physical.staging_admission().unwrap();
        assert_eq!(admission.physical_seal(), physical.seal().unwrap());
        assert_eq!(admission.workspaces().len(), 3);
        assert_eq!(admission.peak_receipt_fds(), 2);
        assert_eq!(admission.retained_fd_slots(), 9);
        assert_eq!(admission.retained_fd_slots_for_containers(1).unwrap(), 7);
        assert_eq!(admission.retained_fd_slots_for_containers(3).unwrap(), 9);
        assert!(admission.retained_fd_slots_for_containers(0).is_err());
        assert!(admission.retained_fd_slots_for_containers(4).is_err());
        assert!(admission.require_fd_slots(8).is_err());
        admission.require_fd_slots(9).unwrap();
        assert_eq!(admission.input_union().entries, 4);
        for w in admission.workspaces() {
            let t = &physical.transactions()[w.transaction];
            let expected = match t.operation() {
                Operation::Add => (2, 2),
                Operation::Replace => (2, 3),
                Operation::Retire => (1, 2),
                _ => panic!("unexpected operation"),
            };
            assert_eq!((w.staged.entries, w.preserved_union.entries), expected);
        }
    }
    #[test]
    fn all_modes_include_copies_replacements_and_explicit_retirements() {
        for mode in [
            Mode::Update(Side::Left),
            Mode::Update(Side::Right),
            Mode::Mirror(Side::Left),
            Mode::Mirror(Side::Right),
            Mode::Merge,
        ] {
            let f = Fixture::new();
            f.file(0, "a", b"left");
            f.file(1, "b", b"right");
            f.file(0, "x", b"old");
            f.file(1, "x", b"new");
            let prepared = f.prepare(mode);
            let record = prepared.record().unwrap();
            let mut p = Plan::new(&record, ignore()).unwrap();
            assert!(Physical::derive(&p).is_err());
            let winner = match mode {
                Mode::Update(s) | Mode::Mirror(s) => s,
                Mode::Merge => Side::Left,
            };
            choose(&mut p, b"x", Decision::Take(winner));
            if let Mode::Mirror(s) = mode {
                choose(
                    &mut p,
                    if s == Side::Left { b"b" } else { b"a" },
                    Decision::Retire,
                );
            }
            let physical = Physical::derive(&p).unwrap();
            let expected = if matches!(mode, Mode::Update(_)) {
                2
            } else {
                3
            };
            assert_eq!(physical.transactions.len(), expected);
            assert_eq!(
                physical
                    .transactions
                    .iter()
                    .filter(|t| t.operation == Operation::Replace)
                    .count(),
                1
            );
            assert_eq!(
                physical
                    .transactions
                    .iter()
                    .filter(|t| t.operation == Operation::Retire)
                    .count(),
                usize::from(matches!(mode, Mode::Mirror(_)))
            );
            for t in &physical.transactions {
                if mode != Mode::Merge {
                    assert_eq!(t.side, 1 - side(winner));
                }
                if t.operation == Operation::Retire {
                    assert!(t.nodes.is_empty());
                    assert_eq!(t.old_items(), 1);
                }
            }
            if mode == Mode::Merge {
                assert_eq!(physical.transactions[0].side, 1);
                assert_eq!(physical.transactions[1].side, 0);
            }
        }
    }
    #[test]
    fn subtree_nodes_have_stage_parents_and_raw_siblings_do_not_leak() {
        let f = Fixture::new();
        fs::create_dir_all(f.paths[0].join("a/dir")).unwrap();
        f.file(0, "a/dir/file", b"x");
        f.file(0, "a-", b"sibling");
        let raw = std::ffi::OsString::from_vec(b".raw\xff\n".to_vec());
        fs::write(f.paths[0].join("a").join(raw), b"raw").unwrap();
        let prepared = f.prepare(Mode::Merge);
        let record = prepared.record().unwrap();
        let p = Plan::new(&record, ignore()).unwrap();
        let physical = Physical::derive(&p).unwrap();
        assert_eq!(physical.transactions.len(), 2);
        assert_eq!(physical.nodes.len(), 5);
        let t = &physical.transactions[0];
        assert_eq!(p.name(t.row), Some(b"a".as_slice()));
        assert_eq!(t.nodes, 0..4);
        assert!(physical.nodes[0].parent_node.is_none());
        for n in &physical.nodes[1..4] {
            assert!(n.parent_node.unwrap() < 4);
        }
        let view = record.view().unwrap();
        for n in &physical.nodes[t.nodes.clone()] {
            assert_ne!(entry(&view, n.source).relative, b"a-");
        }
        assert_eq!(physical.dirty_parents.len(), 1);
    }
    #[test]
    fn replacing_and_retiring_whole_trees_retains_complete_old_scope() {
        let f = Fixture::new();
        f.file(0, "kind", b"file");
        fs::create_dir_all(f.paths[1].join("kind/d")).unwrap();
        f.file(1, "kind/d/x", b"old");
        fs::create_dir_all(f.paths[1].join("gone/d")).unwrap();
        f.file(1, "gone/d/x", b"old");
        f.file(1, "gone-", b"unrelated");
        let prepared = f.prepare(Mode::Mirror(Side::Left));
        let record = prepared.record().unwrap();
        let mut p = Plan::new(&record, ignore()).unwrap();
        choose(&mut p, b"kind", Decision::Take(Side::Left));
        choose(&mut p, b"gone", Decision::Retire);
        choose(&mut p, b"gone-", Decision::Skip);
        let physical = Physical::derive(&p).unwrap();
        assert_eq!(physical.transactions.len(), 2);
        assert_eq!(physical.preserved_entries.len(), 6);
        assert_eq!(physical.transactions[0].backup_entries(), 0..3);
        assert_eq!(physical.transactions[1].backup_entries(), 3..6);
        assert_eq!(physical.transactions[0].old_items(), 3);
        assert!(physical.transactions[0].nodes.is_empty());
        assert_eq!(physical.transactions[1].old_items(), 3);
        assert_eq!(physical.transactions[1].nodes.len(), 1);
        assert_eq!(
            physical.nodes[0].recipe,
            Recipe::Full(physical.nodes[0].source)
        );
    }
    #[test]
    fn metadata_leaf_is_isolated_and_directory_metadata_is_postorder() {
        let f = Fixture::new();
        for s in 0..2 {
            fs::create_dir_all(f.paths[s].join("d/sub")).unwrap();
            f.file(s, "d/sub/x", b"same");
            for name in ["", "d", "d/sub", "d/sub/x"] {
                fs::set_permissions(
                    f.paths[s].join(name),
                    fs::Permissions::from_mode(if s == 0 { 0o700 } else { 0o755 }),
                )
                .unwrap();
            }
        }
        let prepared = f.prepare(Mode::Merge);
        let record = prepared.record().unwrap();
        let mut p = Plan::new(
            &record,
            Options {
                root_metadata: true,
                ..Options::default()
            },
        )
        .unwrap();
        for name in [b"".as_slice(), b"d", b"d/sub", b"d/sub/x"] {
            choose(&mut p, name, Decision::Take(Side::Left));
        }
        let physical = Physical::derive(&p).unwrap();
        assert_eq!(physical.transactions.len(), 1);
        assert_eq!(
            physical.transactions[0].operation,
            Operation::IsolatedMetadata
        );
        assert!(matches!(
            physical.nodes[0].recipe,
            Recipe::Basic { mtime: true, .. }
        ));
        assert_eq!(
            physical
                .directory_metadata
                .iter()
                .map(|d| p.name(d.row).unwrap())
                .collect::<Vec<_>>(),
            vec![b"d/sub".as_slice(), b"d", b""]
        );
        for d in &physical.directory_metadata {
            assert!(matches!(d.recipe, Recipe::Basic { mtime: false, .. }));
        }
    }
    #[test]
    fn policies_preserve_destination_attributes_or_take_full_source_explicitly() {
        let f = Fixture::new();
        f.file(0, "x", b"a");
        f.file(1, "x", b"b");
        let prepared = f.prepare(Mode::Merge);
        let record = prepared.record().unwrap();
        for policy in [Metadata::Ignore, Metadata::Basic, Metadata::ExactObserved] {
            let mut p = Plan::new(
                &record,
                Options {
                    metadata: policy,
                    ..Options::default()
                },
            )
            .unwrap();
            choose(&mut p, b"x", Decision::Take(Side::Left));
            let physical = Physical::derive(&p).unwrap();
            let n = &physical.nodes[0];
            let old = physical.transactions[0].old.unwrap();
            assert_eq!(
                n.recipe,
                match policy {
                    Metadata::Ignore => Recipe::Full(old),
                    Metadata::Basic => Recipe::Basic {
                        base: old,
                        source: n.source,
                        mtime: true
                    },
                    Metadata::ExactObserved => Recipe::Full(n.source),
                }
            );
        }
    }
    #[test]
    fn hardlinks_span_separate_transactions_and_staged_subtrees_but_skips_stay_skipped() {
        let f = Fixture::new();
        fs::create_dir(f.paths[0].join("d")).unwrap();
        f.file(0, "d/x", b"data");
        fs::hard_link(f.paths[0].join("d/x"), f.paths[0].join("outside")).unwrap();
        fs::hard_link(f.paths[0].join("d/x"), f.paths[0].join("skip")).unwrap();
        symlink("missing", f.paths[0].join("link")).unwrap();
        fs::hard_link(f.paths[0].join("link"), f.paths[0].join("link2")).unwrap();
        let prepared = f.prepare(Mode::Merge);
        let record = prepared.record().unwrap();
        let mut p = Plan::new(&record, ignore()).unwrap();
        choose(&mut p, b"skip", Decision::Skip);
        let physical = Physical::derive(&p).unwrap();
        assert_eq!(physical.cohorts.len(), 2);
        assert_eq!(
            physical
                .cohorts
                .iter()
                .map(|g| g.members.len())
                .collect::<Vec<_>>(),
            vec![2, 2]
        );
        assert_eq!(physical.metadata_constraints(), 0);
        assert_eq!(physical.cohorts[0].source_names(), 3);
        assert_eq!(physical.cohorts[0].omitted_sources(), 1);
        let steps = physical.staging_order().unwrap();
        assert_eq!(
            steps
                .iter()
                .filter(|s| matches!(s, StageStep::File { .. }))
                .count(),
            1
        );
        assert_eq!(
            steps
                .iter()
                .filter(|s| matches!(s, StageStep::Symlink { .. }))
                .count(),
            1
        );
        assert_eq!(
            steps
                .iter()
                .filter(|s| matches!(s, StageStep::Link { .. }))
                .count(),
            2
        );
        for s in steps {
            if let StageStep::Link { node, leader } = s {
                assert!(leader < node);
            }
        }
        assert!(
            !physical
                .transactions
                .iter()
                .any(|t| p.name(t.row) == Some(b"skip"))
        );
        let group = &physical.cohorts[0];
        assert_ne!(
            physical.nodes[group.members[0]].transaction,
            physical.nodes[group.members[1]].transaction
        );
        let mut p = Plan::new(
            &record,
            Options {
                hardlinks: Hardlinks::IndependentNames,
                ..ignore()
            },
        )
        .unwrap();
        choose(&mut p, b"skip", Decision::Skip);
        let physical = Physical::derive(&p).unwrap();
        assert!(physical.cohorts.is_empty());
        assert!(physical.nodes.iter().all(|n| n.cohort.is_none()));
    }
    #[test]
    fn hardlink_recipe_mismatch_requires_real_values_never_silent_split() {
        let f = Fixture::new();
        f.file(0, "x", b"source");
        fs::hard_link(f.paths[0].join("x"), f.paths[0].join("y")).unwrap();
        f.file(1, "x", b"old");
        f.file(1, "y", b"old");
        fs::set_permissions(f.paths[1].join("x"), fs::Permissions::from_mode(0o600)).unwrap();
        fs::set_permissions(f.paths[1].join("y"), fs::Permissions::from_mode(0o644)).unwrap();
        let prepared = f.prepare(Mode::Merge);
        let record = prepared.record().unwrap();
        for policy in [Metadata::Ignore, Metadata::Basic, Metadata::ExactObserved] {
            let mut p = Plan::new(
                &record,
                Options {
                    metadata: policy,
                    ..Options::default()
                },
            )
            .unwrap();
            choose(&mut p, b"x", Decision::Take(Side::Left));
            choose(&mut p, b"y", Decision::Take(Side::Left));
            let physical = Physical::derive(&p).unwrap();
            assert_eq!(physical.cohorts.len(), 1);
            assert_eq!(physical.cohorts[0].members.len(), 2);
            assert_eq!(
                physical.metadata_constraints(),
                usize::from(policy != Metadata::ExactObserved)
            );
        }
    }
    #[test]
    fn old_shared_inode_aliases_are_recorded_without_mutating_outside_data() {
        let f = Fixture::new();
        f.file(0, "x", b"new");
        f.file(1, "x", b"old");
        fs::hard_link(f.paths[1].join("x"), f.paths[1].join("keep")).unwrap();
        fs::hard_link(f.paths[1].join("x"), f.base.join("external")).unwrap();
        let prepared = f.prepare(Mode::Update(Side::Left));
        let record = prepared.record().unwrap();
        let mut p = Plan::new(&record, ignore()).unwrap();
        choose(&mut p, b"x", Decision::Take(Side::Left));
        let physical = Physical::derive(&p).unwrap();
        assert_eq!(physical.aliases.len(), 1);
        assert_eq!(physical.aliases[0].captured.len(), 2);
        assert_eq!(physical.aliases[0].external, 1);
        assert_eq!(fs::read(f.base.join("external")).unwrap(), b"old");
        assert_eq!(fs::read(f.paths[1].join("x")).unwrap(), b"old");
    }
    fn altered_mount(record: &Record, s: usize, name: &[u8], mount: u64) -> Record {
        let view = record.view().unwrap();
        let e = &view.roots[s].entries[locate(&view, s, name).unwrap().index];
        let offset = e.relative.as_ptr() as usize - record.bytes().as_ptr() as usize
            + e.relative.len()
            + FIXED_ENTRY
            - 2
            - 8;
        let mut bytes = record.bytes().to_vec();
        bytes[offset..offset + 8].copy_from_slice(&mount.to_le_bytes());
        let end = bytes.len() - 32;
        let mut h = Sha256::new();
        h.update(b"Files:sync-evidence:v1\0");
        h.update(&bytes[..end]);
        bytes[end..].copy_from_slice(&h.finalize());
        Record::decode(&bytes).unwrap()
    }
    #[test]
    fn destination_root_or_nested_mount_boundary_blocks_backup() {
        let f = Fixture::new();
        f.file(0, "x", b"a");
        fs::create_dir(f.paths[1].join("x")).unwrap();
        f.file(1, "x/child", b"b");
        let prepared = f.prepare(Mode::Merge);
        let record = prepared.record().unwrap();
        let mount = record.view().unwrap().roots[1].entries[0].mount + 1;
        for name in [b"x".as_slice(), b"x/child"] {
            let altered = altered_mount(&record, 1, name, mount);
            let mut p = Plan::new(&altered, ignore()).unwrap();
            choose(&mut p, b"x", Decision::Take(Side::Left));
            assert!(
                Physical::derive(&p)
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("montaje")
            );
        }
    }
    #[test]
    fn cross_mount_cohorts_block_linking_but_explicit_independent_names_can_plan() {
        let f = Fixture::new();
        for s in 0..2 {
            for name in ["one", "two"] {
                fs::create_dir(f.paths[s].join(name)).unwrap();
            }
        }
        f.file(0, "one/x", b"same");
        fs::hard_link(f.paths[0].join("one/x"), f.paths[0].join("two/y")).unwrap();
        let prepared = f.prepare(Mode::Merge);
        let record = prepared.record().unwrap();
        let mount = record.view().unwrap().roots[1].entries[0].mount + 1;
        let altered = altered_mount(&record, 1, b"two", mount);
        let p = Plan::new(&altered, ignore()).unwrap();
        assert!(
            Physical::derive(&p)
                .err()
                .unwrap()
                .to_string()
                .contains("hardlink")
        );
        let p = Plan::new(
            &altered,
            Options {
                hardlinks: Hardlinks::IndependentNames,
                ..ignore()
            },
        )
        .unwrap();
        let physical = Physical::derive(&p).unwrap();
        assert_eq!(physical.transactions.len(), 2);
        assert_ne!(
            physical.transactions[0].mount,
            physical.transactions[1].mount
        );
    }
    #[test]
    fn canonical_blueprint_changes_with_choices_and_preserves_empty_plan() {
        let f = Fixture::new();
        let prepared = f.prepare(Mode::Merge);
        let record = prepared.record().unwrap();
        let p = Plan::new(&record, ignore()).unwrap();
        let a = Physical::derive(&p).unwrap();
        assert!(a.nodes.is_empty());
        assert_eq!(
            a.seal().unwrap(),
            Physical::derive(&p).unwrap().seal().unwrap()
        );
        drop(a);
        drop(p);
        drop(record);
        drop(prepared);
        f.file(0, "x", b"x");
        let prepared = f.prepare(Mode::Merge);
        let record = prepared.record().unwrap();
        let mut p = Plan::new(&record, ignore()).unwrap();
        let before = Physical::derive(&p).unwrap().seal().unwrap();
        choose(&mut p, b"x", Decision::Skip);
        assert_ne!(before, Physical::derive(&p).unwrap().seal().unwrap());
    }
    #[test]
    fn independent_hardlink_copies_charge_payload_multiplication_before_any_effect() {
        let f = Fixture::new();
        f.file(0, "a", &[1; 1024]);
        for name in ["b", "c", "d"] {
            fs::hard_link(f.paths[0].join("a"), f.paths[0].join(name)).unwrap();
        }
        let prepared = f.prepare(Mode::Merge);
        let record = prepared.record().unwrap();
        let p = Plan::new(&record, ignore()).unwrap();
        let linked = Physical::derive_with_limit(&p, 1024).unwrap();
        assert_eq!(linked.stage_payload(), 1024);
        let old_seal = linked.seal().unwrap();
        assert_ne!(
            old_seal,
            Physical::derive_with_limit(&p, 4096)
                .unwrap()
                .seal()
                .unwrap()
        );
        let p = Plan::new(
            &record,
            Options {
                hardlinks: Hardlinks::IndependentNames,
                ..ignore()
            },
        )
        .unwrap();
        assert!(
            Physical::derive_with_limit(&p, 1024)
                .err()
                .unwrap()
                .to_string()
                .contains("Payload")
        );
        assert_eq!(
            Physical::derive_with_limit(&p, 4096)
                .unwrap()
                .stage_payload(),
            4096
        );
        assert!(Physical::derive_with_limit(&p, MAX_STAGE_PAYLOAD + 1).is_err());
        assert_eq!(fs::read(f.paths[0].join("a")).unwrap(), vec![1; 1024]);
        assert!(!f.paths[1].join("a").exists());
    }
    #[test]
    fn symlink_target_payload_is_charged_and_zero_payload_directories_are_allowed() {
        let f = Fixture::new();
        fs::create_dir(f.paths[0].join("empty")).unwrap();
        let prepared = f.prepare(Mode::Merge);
        let record = prepared.record().unwrap();
        let p = Plan::new(&record, ignore()).unwrap();
        assert_eq!(
            Physical::derive_with_limit(&p, 0).unwrap().stage_payload(),
            0
        );
        drop(p);
        drop(record);
        drop(prepared);
        symlink("missing", f.paths[0].join("link")).unwrap();
        let prepared = f.prepare(Mode::Merge);
        let record = prepared.record().unwrap();
        let p = Plan::new(&record, ignore()).unwrap();
        assert!(Physical::derive_with_limit(&p, 6).is_err());
        assert_eq!(
            Physical::derive_with_limit(&p, 7).unwrap().stage_payload(),
            7
        );
    }
    #[test]
    fn full_both_sides_expand_without_truncation_or_duplicate_names() {
        let f = Fixture::new();
        for s in 0..2 {
            for n in 0..crate::directory_compare::MAX_ITEMS {
                f.file(s, &format!("{s}-{n:04}-{}", "x".repeat(118)), b"");
            }
        }
        let prepared = f.prepare(Mode::Merge);
        let record = prepared.record().unwrap();
        let p = Plan::new(&record, ignore()).unwrap();
        let physical = Physical::derive(&p).unwrap();
        assert_eq!(physical.transactions.len(), MAX_TRANSACTIONS);
        assert_eq!(physical.nodes.len(), MAX_NODES);
        assert_eq!(physical.cohorts.len(), MAX_NODES);
        assert_eq!(physical.dirty_parents.len(), 2);
        let encoded = physical.encode().unwrap();
        assert!(encoded.len() < MAX_WIRE);
        assert_eq!(encoded, Physical::derive(&p).unwrap().encode().unwrap());
    }
}
