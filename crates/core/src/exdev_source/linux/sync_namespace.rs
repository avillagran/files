//! Expected namespace reconciliation; own syscalls use the journal-gated route.
use super::*;
use crate::directory_sync_metadata::{EvidenceTemplate, Time};
use std::collections::HashSet;
mod own;
pub use own::NamespaceFailure;
pub(crate) use own::{NamespaceControl, NamespaceLog, OwnPoint};
/// Whole-job observer profile; original per-scope capture limits remain unchanged.
pub const MAX_BATCH_NAMESPACE_SCOPES: usize = crate::directory_sync_physical::MAX_TRANSACTIONS + 2;
pub const MAX_BATCH_NAMESPACE_ENTRIES: usize = 3 * crate::directory_sync_physical::MAX_NODES + 2;
pub const MAX_BATCH_NAMESPACE_NAMES: usize =
    4 * MAX_NAMESPACE_NAMES + 4 * crate::directory_sync_physical::MAX_NODES;
pub const MAX_BATCH_NAMESPACE_WIRE: usize =
    MAX_BATCH_NAMESPACE_ENTRIES * (148 + 255) + MAX_BATCH_NAMESPACE_NAMES;
#[derive(Clone, Copy)]
enum Profile {
    Endpoints,
    Batch,
}
#[derive(Default)]
struct BatchBudget {
    entries: usize,
    names: usize,
    wire: usize,
}
impl BatchBudget {
    fn charge(&mut self, len: usize, root: usize) -> io::Result<()> {
        let add = |value: usize, increment: usize, max: usize| {
            value
                .checked_add(increment)
                .filter(|&v| v <= max)
                .ok_or_else(|| invalid("Lote de namespace excedido"))
        };
        self.entries = add(self.entries, 1, MAX_BATCH_NAMESPACE_ENTRIES)?;
        self.names = add(
            self.names,
            len.saturating_sub(root + 1),
            MAX_BATCH_NAMESPACE_NAMES,
        )?;
        self.wire = add(self.wire, 147 + len, MAX_BATCH_NAMESPACE_WIRE)?;
        Ok(())
    }
}
pub struct NamespaceScope<'a> {
    pub inventory: &'a mut Inventory,
    pub parent: &'a File,
    pub name: &'a OsStr,
}
#[derive(Clone, Copy)]
pub struct NamespacePath<'a> {
    pub scope: usize,
    pub relative: &'a [u8],
}
struct Candidate {
    entries: Vec<Entry>,
    mounts: Vec<u64>,
}
#[derive(Clone, Copy, Default)]
struct CandidateBudget {
    count: usize,
    names: usize,
    wire: usize,
}
impl CandidateBudget {
    fn charge(&mut self, prefix: &[u8], suffix: &[u8], root_prefix: usize) -> io::Result<()> {
        let len = prefix
            .len()
            .checked_add(suffix.len())
            .ok_or_else(|| invalid("Ruta resultante excedida"))?;
        let relative = len.saturating_sub(root_prefix + 1);
        let depth = prefix.iter().chain(suffix).filter(|&&b| b == b'/').count();
        if relative > MAX_PATH || depth >= crate::directory_compare::MAX_DEPTH + 2 {
            return Err(invalid("Ruta/profundidad resultante excedida"));
        }
        self.count = self
            .count
            .checked_add(1)
            .filter(|&n| n <= MAX_NAMESPACE_ENTRIES)
            .ok_or_else(|| invalid("Conteo de namespace excedido"))?;
        self.names = self
            .names
            .checked_add(relative)
            .filter(|&n| n <= MAX_NAMESPACE_NAMES)
            .ok_or_else(|| invalid("Nombres resultantes excedidos"))?;
        self.wire = self
            .wire
            .checked_add(147 + len)
            .filter(|&n| n <= MAX_WIRE * 4 - 16384)
            .ok_or_else(|| invalid("Wire de namespace excedido"))?;
        Ok(())
    }
}
struct ParentProof {
    scope: usize,
    index: usize,
    template: EvidenceTemplate,
    links: u64,
}
/// Borrowed before/after plan, not operator approval or proof of rename provenance.
/// Drop/failure never repairs filesystem state. The original owners stay unchanged
/// until every scope has passed post-rename verification.
pub struct NamespaceMove<'a> {
    scopes: Vec<NamespaceScope<'a>>,
    candidates: Vec<Candidate>,
    parents: Vec<ParentProof>,
    change: RenameChange,
    preparation_read: u64,
    endpoints: own::Endpoints,
}
fn relative(path: &[u8]) -> io::Result<()> {
    if path.is_empty() || path.len() > MAX_PATH {
        return Err(invalid("Ruta relativa de rename inválida"));
    }
    for part in path.split(|&b| b == b'/') {
        component(part)?;
    }
    Ok(())
}
fn full(name: &OsStr, path: &[u8]) -> Vec<u8> {
    let mut out = name.as_bytes().to_vec();
    if !path.is_empty() {
        out.push(b'/');
        out.extend_from_slice(path);
    }
    out
}
fn parent_path(path: &[u8]) -> &[u8] {
    path.iter()
        .rposition(|&b| b == b'/')
        .map_or(&[], |i| &path[..i])
}
fn under(path: &[u8], root: &[u8]) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|p| p.first() == Some(&b'/'))
}
fn copy_entry(e: &Entry, path: Vec<u8>) -> Entry {
    Entry {
        relative: path,
        identity: e.identity,
        revision: e.revision,
        access: e.access,
        content_digest: e.content_digest,
        metadata_digest: e.metadata_digest,
    }
}
fn child_digest(entries: &[Entry], parent: &[u8]) -> [u8; 32] {
    let mut names: Vec<_> = entries
        .iter()
        .filter_map(|e| {
            let i = e.relative.iter().rposition(|&b| b == b'/')?;
            (&e.relative[..i] == parent).then_some(&e.relative[i + 1..])
        })
        .collect();
    names.sort_unstable();
    let mut h = Sha256::new();
    for n in names {
        sized(&mut h, n);
    }
    h.finalize().into()
}
fn links_after(inv: &Inventory, parent: usize, delta: i8) -> io::Result<u64> {
    let e = &inv.entries[parent];
    if delta == 0 || e.revision.links == 1 {
        return Ok(e.revision.links);
    }
    let dirs = inv
        .entries
        .iter()
        .filter(|c| c.identity.kind == Kind::Directory && parent_path(&c.relative) == e.relative)
        .count() as u64;
    if e.revision.links != 2 + dirs {
        return Err(invalid("Modelo de nlink de directorio no reconocido"));
    }
    if delta < 0 {
        e.revision.links.checked_sub(1)
    } else {
        e.revision.links.checked_add(1)
    }
    .ok_or_else(|| invalid("nlink de directorio fuera de rango"))
}
/// Read-only mutable-execution snapshot; original input capture caps stay unchanged.
pub fn capture_namespace(
    parent: &File,
    name: &OsStr,
    read: u64,
    cancel: &AtomicBool,
) -> io::Result<Inventory> {
    capture_namespace_inner(parent, name, read, cancel, None)
}
pub fn capture_namespace_controlled(
    parent: &File,
    name: &OsStr,
    read: u64,
    control: &Control,
) -> io::Result<Inventory> {
    capture_namespace_inner(
        parent,
        name,
        read,
        control.cancellation_flag(),
        Some(control),
    )
}
fn capture_namespace_inner(
    parent: &File,
    name: &OsStr,
    read: u64,
    cancel: &AtomicBool,
    control: Option<&Control>,
) -> io::Result<Inventory> {
    if read > crate::directory_compare::MAX_READ {
        return Err(invalid("Allowance de captura namespace inválido"));
    }
    let inv = capture_options(
        parent,
        name,
        cancel,
        control,
        Some(SyncBounds {
            read,
            prefix: name.as_bytes().len(),
            namespace: true,
        }),
    )?;
    if inv.entries[0].identity.kind != Kind::Directory {
        return Err(invalid("Namespace requiere raíz directorio"));
    }
    Ok(inv)
}
pub fn plan_namespace_move<'a>(
    scopes: Vec<NamespaceScope<'a>>,
    from: NamespacePath<'_>,
    to: NamespacePath<'_>,
    read: u64,
    cancel: &AtomicBool,
) -> io::Result<NamespaceMove<'a>> {
    prepare(scopes, from, to, read, cancel, None, Profile::Endpoints)
}
pub fn plan_namespace_move_controlled<'a>(
    scopes: Vec<NamespaceScope<'a>>,
    from: NamespacePath<'_>,
    to: NamespacePath<'_>,
    read: u64,
    control: &Control,
) -> io::Result<NamespaceMove<'a>> {
    prepare(
        scopes,
        from,
        to,
        read,
        control.cancellation_flag(),
        Some(control),
        Profile::Endpoints,
    )
}
/// Include every affected alias owner in one atomic inventory reconciliation.
/// Observer scopes are fully checked; this is not authorization to publish.
pub fn plan_batch_namespace_move<'a>(
    scopes: Vec<NamespaceScope<'a>>,
    from: NamespacePath<'_>,
    to: NamespacePath<'_>,
    read: u64,
    cancel: &AtomicBool,
) -> io::Result<NamespaceMove<'a>> {
    prepare(scopes, from, to, read, cancel, None, Profile::Batch)
}
pub fn plan_batch_namespace_move_controlled<'a>(
    scopes: Vec<NamespaceScope<'a>>,
    from: NamespacePath<'_>,
    to: NamespacePath<'_>,
    read: u64,
    control: &Control,
) -> io::Result<NamespaceMove<'a>> {
    prepare(
        scopes,
        from,
        to,
        read,
        control.cancellation_flag(),
        Some(control),
        Profile::Batch,
    )
}
#[allow(clippy::too_many_arguments)]
fn prepare<'a>(
    scopes: Vec<NamespaceScope<'a>>,
    from: NamespacePath<'_>,
    to: NamespacePath<'_>,
    read: u64,
    cancel: &AtomicBool,
    control: Option<&Control>,
    profile: Profile,
) -> io::Result<NamespaceMove<'a>> {
    check_control(cancel, control)?;
    relative(from.relative)?;
    relative(to.relative)?;
    if read > crate::directory_compare::MAX_READ
        || scopes.is_empty()
        || scopes.len()
            > match profile {
                Profile::Endpoints => 2,
                Profile::Batch => MAX_BATCH_NAMESPACE_SCOPES,
            }
        || from.scope >= scopes.len()
        || to.scope >= scopes.len()
        || (matches!(profile, Profile::Endpoints)
            && scopes.len() != if from.scope == to.scope { 1 } else { 2 })
    {
        return Err(invalid("Scopes de rename inválidos"));
    }
    // Admit ALL borrowed input shapes before allocating directory sets or candidates.
    if matches!(profile, Profile::Batch) {
        let mut total = BatchBudget::default();
        for s in &scopes {
            let mut local = CandidateBudget::default();
            for e in &s.inventory.entries {
                local.charge(&e.relative, &[], s.name.as_bytes().len())?;
                total.charge(e.relative.len(), s.name.as_bytes().len())?;
            }
        }
    }
    let mut dirs = HashSet::new();
    for s in &scopes {
        component(s.name.as_bytes())?;
        if s.inventory.sync.is_none()
            || s.inventory.entries.first().is_none_or(|e| {
                e.identity.kind != Kind::Directory || e.relative != s.name.as_bytes()
            })
            || s.inventory
                .mounts
                .as_ref()
                .is_none_or(|m| m.len() != s.inventory.entries.len())
        {
            return Err(invalid("Scope requiere raíz directorio sync completa"));
        }
        for e in s
            .inventory
            .entries
            .iter()
            .filter(|e| e.identity.kind == Kind::Directory)
        {
            if !dirs.insert((e.identity.device, e.identity.inode)) {
                return Err(invalid("Scopes de directorio solapados"));
            }
        }
    }
    let source = &scopes[from.scope];
    let destination = &scopes[to.scope];
    let old_path = full(source.name, from.relative);
    let new_path = full(destination.name, to.relative);
    let source_index = source
        .inventory
        .entries
        .binary_search_by(|e| e.relative.cmp(&old_path))
        .map_err(|_| invalid("Origen de rename ausente"))?;
    if destination
        .inventory
        .entries
        .binary_search_by(|e| e.relative.cmp(&new_path))
        .is_ok()
    {
        return Err(invalid("Destino de rename ya existe"));
    }
    let sp = full(source.name, parent_path(from.relative));
    let dp = full(destination.name, parent_path(to.relative));
    let si = source
        .inventory
        .entries
        .binary_search_by(|e| e.relative.cmp(&sp))
        .map_err(|_| invalid("Padre origen ausente"))?;
    let di = destination
        .inventory
        .entries
        .binary_search_by(|e| e.relative.cmp(&dp))
        .map_err(|_| invalid("Padre destino ausente"))?;
    let moved = &source.inventory.entries[source_index];
    if source.inventory.entries[si].identity.kind != Kind::Directory
        || destination.inventory.entries[di].identity.kind != Kind::Directory
        || from.scope == to.scope && under(&dp, &old_path)
    {
        return Err(invalid("Padre inválido o ciclo en rename"));
    }
    let mount = source.inventory.mounts.as_ref().expect("checked")[source_index];
    if source.inventory.mounts.as_ref().expect("checked")[si] != mount
        || destination.inventory.mounts.as_ref().expect("checked")[di] != mount
    {
        return Err(invalid("Rename entre montajes no permitido"));
    }
    for s in &scopes {
        for (e, m) in s
            .inventory
            .entries
            .iter()
            .zip(s.inventory.mounts.as_ref().expect("checked"))
        {
            if e.identity == moved.identity
                && (e.revision != moved.revision
                    || e.access != moved.access
                    || e.content_digest != moved.content_digest
                    || e.metadata_digest != moved.metadata_digest
                    || *m != mount)
            {
                return Err(invalid("Capturas de inode incoherentes"));
            }
        }
    }
    let change = RenameChange {
        identity: moved.identity,
        before: moved.revision,
        after: moved.revision,
        access: moved.access,
        content: moved.content_digest,
        metadata: moved.metadata_digest,
        mount,
        payload: 0,
    };
    let mut pairs: Vec<Vec<(Entry, u64)>> = (0..scopes.len()).map(|_| Vec::new()).collect();
    let mut budgets = vec![CandidateBudget::default(); scopes.len()];
    let mut total = BatchBudget::default();
    for (scope, s) in scopes.iter().enumerate() {
        for (e, &m) in s
            .inventory
            .entries
            .iter()
            .zip(s.inventory.mounts.as_ref().expect("checked"))
        {
            let (target, prefix, suffix) = if scope == from.scope && under(&e.relative, &old_path) {
                (to.scope, new_path.as_slice(), &e.relative[old_path.len()..])
            } else {
                (scope, e.relative.as_slice(), &[][..])
            };
            // Charge the resulting shape BEFORE cloning or reserving its path.
            budgets[target].charge(prefix, suffix, scopes[target].name.as_bytes().len())?;
            if matches!(profile, Profile::Batch) {
                total.charge(
                    prefix.len() + suffix.len(),
                    scopes[target].name.as_bytes().len(),
                )?;
            }
            let mut path = prefix.to_vec();
            path.extend_from_slice(suffix);
            pairs[target].push((copy_entry(e, path), m));
        }
    }
    let mut candidates = Vec::with_capacity(scopes.len());
    for mut pairs in pairs {
        if pairs.is_empty() {
            return Err(invalid("Namespace sin raíz"));
        }
        pairs.sort_unstable_by(|a, b| a.0.relative.cmp(&b.0.relative));
        if pairs.windows(2).any(|w| w[0].0.relative == w[1].0.relative) {
            return Err(invalid("Nombre resultante duplicado"));
        }
        let (entries, mounts) = pairs.into_iter().unzip();
        candidates.push(Candidate { entries, mounts });
    }
    // Fully validate originals before acquiring compact directory templates.
    let mut spent = 0;
    for s in &scopes {
        spent +=
            s.inventory
                .verify_observed(s.parent, s.name, cancel, control, Some(read - spent))?;
    }
    let delta = if change.identity.kind == Kind::Directory && (from.scope, si) != (to.scope, di) {
        1
    } else {
        0
    };
    let mut drafts = vec![(from.scope, si, -delta)];
    if (from.scope, si) != (to.scope, di) {
        drafts.push((to.scope, di, delta));
    }
    let mut parents = Vec::new();
    for (scope, index, delta) in drafts {
        check_control(cancel, control)?;
        let s = &scopes[scope];
        let e = &s.inventory.entries[index];
        let template = s
            .inventory
            .metadata_values_for_sync(s.parent, s.name, index, cancel)?
            .evidence_template();
        let links = links_after(s.inventory, index, delta)?;
        let candidate_index = candidates[scope]
            .entries
            .iter()
            .position(|c| c.identity == e.identity)
            .ok_or_else(|| invalid("Padre resultante ausente"))?;
        let digest = child_digest(&candidates[scope].entries, &e.relative);
        candidates[scope].entries[candidate_index].content_digest = digest;
        parents.push(ParentProof {
            scope,
            index: candidate_index,
            template,
            links,
        });
    }
    for s in &scopes {
        spent +=
            s.inventory
                .verify_observed(s.parent, s.name, cancel, control, Some(read - spent))?;
    }
    check_control(cancel, control)?;
    let endpoints = own::Endpoints::new(
        &scopes,
        from,
        to,
        source_index,
        si,
        di,
        &old_path,
        &new_path,
    );
    Ok(NamespaceMove {
        endpoints,
        scopes,
        candidates,
        parents,
        change,
        preparation_read: spent,
    })
}
impl NamespaceMove<'_> {
    fn check_current_roots(
        &self,
        cancel: &AtomicBool,
        control: Option<&Control>,
    ) -> io::Result<()> {
        for (s, c) in self.scopes.iter().zip(&self.candidates) {
            check_control(cancel, control)?;
            let e = &c.entries[0];
            stable(s.parent, s.name, &s.inventory.root, e.identity, e.revision)?;
            if mount_id(&s.inventory.root, None)? != c.mounts[0]
                || mount_id(s.parent, Some(s.name))? != c.mounts[0]
            {
                return Err(invalid("Montaje raíz de namespace cambió"));
            }
        }
        check_control(cancel, control)
    }
    pub fn preparation_read(&self) -> u64 {
        self.preparation_read
    }
    pub fn reconcile(self, read: u64, cancel: &AtomicBool) -> io::Result<RenameChange> {
        self.reconcile_inner(read, cancel, None, |_| Ok(()))
    }
    pub fn reconcile_controlled(self, read: u64, control: &Control) -> io::Result<RenameChange> {
        self.reconcile_inner(read, control.cancellation_flag(), Some(control), |_| Ok(()))
    }
    fn reconcile_inner(
        mut self,
        read: u64,
        cancel: &AtomicBool,
        control: Option<&Control>,
        finish: impl FnOnce(&Self) -> io::Result<()>,
    ) -> io::Result<RenameChange> {
        check_control(cancel, control)?;
        if read > crate::directory_compare::MAX_READ {
            return Err(invalid("Allowance de namespace inválido"));
        }
        let mut parent_revisions = vec![None; self.parents.len()];
        let mut moved_revision = None;
        let mut payload = 0;
        for (scope, (s, candidate)) in self.scopes.iter().zip(&self.candidates).enumerate() {
            let expected: HashMap<&[u8], &Entry> = candidate
                .entries
                .iter()
                .map(|e| (e.relative.as_slice(), e))
                .collect();
            let root = open(s.parent, s.name)?;
            if id(&root.metadata()?)? != candidate.entries[0].identity
                || id(&s.inventory.root.metadata()?)? != candidate.entries[0].identity
            {
                return Err(invalid("Raíz de namespace sustituida"));
            }
            let mut count = 0;
            let mut walker = Walker::with_control(cancel, Some(&expected), control, |observed| {
                let old = *expected
                    .get(observed.relative.as_slice())
                    .ok_or_else(|| invalid("Nombre ajeno al rename previsto"))?;
                if let Some((index, parent)) = self.parents.iter().enumerate().find(|(_, p)| {
                    p.scope == scope && candidate.entries[p.index].identity == observed.identity
                }) {
                    let r = observed.revision;
                    let time = Time {
                        seconds: r.mtime_seconds,
                        nanos: r.mtime_nanos,
                    };
                    if observed.identity != old.identity
                        || observed.access != old.access
                        || observed.content_digest != old.content_digest
                        || r.mode != old.revision.mode
                        || r.uid != old.revision.uid
                        || r.gid != old.revision.gid
                        || r.links != parent.links
                        || observed.metadata_digest != parent.template.at(time)
                    {
                        return Err(invalid("Padre cambió además del namespace previsto"));
                    }
                    parent_revisions[index] = Some(r);
                } else if old.identity == self.change.identity {
                    if observed.identity != old.identity
                        || observed.access != old.access
                        || observed.content_digest != old.content_digest
                        || observed.metadata_digest != old.metadata_digest
                        || !observed.revision.matches_after_rename(old.revision)
                        || moved_revision.is_some_and(|r| r != observed.revision)
                    {
                        return Err(invalid("Objeto movido cambió además del rename"));
                    }
                    moved_revision = Some(observed.revision);
                } else if &observed != old {
                    return Err(invalid("Objeto ajeno al rename cambió"));
                }
                count += 1;
                Ok(())
            });
            walker.set_sync(Some(SyncBounds {
                read: read - payload,
                prefix: s.name.as_bytes().len(),
                namespace: true,
            }));
            walker.walk(s.parent, s.name, root, s.name.as_bytes(), 0)?;
            payload += walker.payload_read;
            let topology = walker
                .topology
                .take()
                .ok_or_else(|| invalid("Namespace sin montajes"))?;
            drop(walker);
            if count != candidate.entries.len()
                || candidate
                    .entries
                    .iter()
                    .zip(&candidate.mounts)
                    .any(|(e, m)| topology.get(&e.relative) != Some(m))
            {
                return Err(invalid("Namespace incompleto o montaje distinto"));
            }
        }
        self.change.after = moved_revision.ok_or_else(|| invalid("Objeto movido no observado"))?;
        self.change.payload = payload;
        for (i, p) in self.parents.iter().enumerate() {
            let revision = parent_revisions[i].ok_or_else(|| invalid("Padre no observado"))?;
            let e = &mut self.candidates[p.scope].entries[p.index];
            e.revision = revision;
            e.metadata_digest = p.template.at(Time {
                seconds: revision.mtime_seconds,
                nanos: revision.mtime_nanos,
            });
        }
        for candidate in &mut self.candidates {
            for e in &mut candidate.entries {
                if e.identity == self.change.identity {
                    e.revision = self.change.after;
                }
            }
        }
        for (s, c) in self.scopes.iter().zip(&self.candidates) {
            check_control(cancel, control)?;
            let e = &c.entries[0];
            stable(s.parent, s.name, &s.inventory.root, e.identity, e.revision)?;
            if mount_id(&s.inventory.root, None)? != c.mounts[0]
                || mount_id(s.parent, Some(s.name))? != c.mounts[0]
            {
                return Err(invalid("Montaje raíz de namespace cambió"));
            }
        }
        check_control(cancel, control)?;
        finish(&self)?;
        self.check_current_roots(cancel, control)?;
        // No fallible operations remain; publish both recorded owners together.
        for (s, c) in self.scopes.iter_mut().zip(self.candidates) {
            s.inventory.entries = c.entries;
            s.inventory.mounts = Some(c.mounts);
            s.inventory.sync = s.inventory.sync.map(|b| SyncBounds {
                namespace: true,
                ..b
            });
        }
        Ok(self.change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::unix::fs::{PermissionsExt, symlink},
        path::PathBuf,
    };
    struct Fixture {
        base: PathBuf,
        parent: File,
        a: Inventory,
        b: Inventory,
    }
    impl Fixture {
        fn new(directory: bool) -> Self {
            let base = crate::persistence::test_dir("sync-namespace");
            for p in ["a/one", "a/two", "b/two"] {
                fs::create_dir_all(base.join(p)).unwrap();
                fs::set_permissions(base.join(p), fs::Permissions::from_mode(0o755)).unwrap();
            }
            if directory {
                for p in ["a/one/item/a-", "a/one/item/a", "a/one/item/empty"] {
                    fs::create_dir_all(base.join(p)).unwrap();
                }
                fs::write(base.join("a/one/item/a-/leaf"), b"data").unwrap();
                fs::hard_link(
                    base.join("a/one/item/a-/leaf"),
                    base.join("a/one/item/a/child"),
                )
                .unwrap();
                symlink("a-/leaf", base.join("a/one/item/link")).unwrap();
            } else {
                fs::write(base.join("a/one/item"), b"data").unwrap();
                for p in ["a/alias", "b/alias", "outside"] {
                    fs::hard_link(base.join("a/one/item"), base.join(p)).unwrap();
                }
            }
            for p in ["a/unrelated", "b/unrelated"] {
                fs::write(base.join(p), b"other").unwrap();
                fs::set_permissions(base.join(p), fs::Permissions::from_mode(0o644)).unwrap();
            }
            let parent = File::open(&base).unwrap();
            let cancel = AtomicBool::new(false);
            let a = capture_for_sync(&parent, OsStr::new("a"), 64, &cancel).unwrap();
            let b = capture_for_sync(&parent, OsStr::new("b"), 64, &cancel).unwrap();
            Self { base, parent, a, b }
        }
        fn scopes(&mut self, cross: bool) -> Vec<NamespaceScope<'_>> {
            let mut scopes = vec![NamespaceScope {
                inventory: &mut self.a,
                parent: &self.parent,
                name: OsStr::new("a"),
            }];
            if cross {
                scopes.push(NamespaceScope {
                    inventory: &mut self.b,
                    parent: &self.parent,
                    name: OsStr::new("b"),
                });
            }
            scopes
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }
    fn path(scope: usize, relative: &[u8]) -> NamespacePath<'_> {
        NamespacePath { scope, relative }
    }
    #[test]
    fn batch_namespace_updates_all_observers_and_composes_next_move() {
        for symlink_inode in [false, true] {
            let f = Fixture::new(true);
            let cancel = AtomicBool::new(false);
            for root in ["c", "d"] {
                fs::create_dir(f.base.join(root)).unwrap();
            }
            if symlink_inode {
                symlink("target", f.base.join("a/file")).unwrap();
            } else {
                fs::write(f.base.join("a/file"), b"payload").unwrap();
            }
            for alias in ["b/alias2", "c/alias", "d/alias"] {
                fs::hard_link(f.base.join("a/file"), f.base.join(alias)).unwrap();
            }
            let names = ["a", "b", "c", "d"];
            let mut inventories: Vec<_> = names
                .iter()
                .map(|n| capture_namespace(&f.parent, OsStr::new(n), 128, &cancel).unwrap())
                .collect();
            // The endpoint-only API deliberately retains its original shape restriction.
            let scopes = inventories
                .iter_mut()
                .zip(names)
                .map(|(inventory, name)| NamespaceScope {
                    inventory,
                    parent: &f.parent,
                    name: OsStr::new(name),
                })
                .collect();
            assert!(
                plan_namespace_move(scopes, path(0, b"file"), path(1, b"new"), 256, &cancel)
                    .is_err()
            );
            for (from, source, to, target) in [
                (0, b"file".as_slice(), 1, b"new".as_slice()),
                (1, b"new", 2, b"published"),
            ] {
                let scopes = inventories
                    .iter_mut()
                    .zip(names)
                    .map(|(inventory, name)| NamespaceScope {
                        inventory,
                        parent: &f.parent,
                        name: OsStr::new(name),
                    })
                    .collect();
                let plan = plan_batch_namespace_move_controlled(
                    scopes,
                    path(from, source),
                    path(to, target),
                    256,
                    &Control::default(),
                )
                .unwrap();
                // This test exercises expected reconciliation, not journal authorization.
                fs::rename(
                    f.base.join(names[from]).join(OsStr::from_bytes(source)),
                    f.base.join(names[to]).join(OsStr::from_bytes(target)),
                )
                .unwrap();
                plan.reconcile(128, &cancel).unwrap();
                for (inv, name) in inventories.iter().zip(names) {
                    inv.verify_for_sync(&f.parent, OsStr::new(name), 128, &cancel)
                        .unwrap();
                }
            }
        }
    }
    #[test]
    fn batch_namespace_late_observer_failure_keeps_every_owner_unchanged() {
        for failure in ["content", "root", "cancel", "budget"] {
            let f = Fixture::new(false);
            let cancel = AtomicBool::new(false);
            fs::create_dir(f.base.join("c")).unwrap();
            fs::hard_link(f.base.join("a/one/item"), f.base.join("c/alias")).unwrap();
            fs::write(f.base.join("c/unrelated"), b"other").unwrap();
            let names = ["a", "b", "c"];
            let mut inventories: Vec<_> = names
                .iter()
                .map(|n| capture_namespace(&f.parent, OsStr::new(n), 128, &cancel).unwrap())
                .collect();
            let before: Vec<_> = inventories
                .iter()
                .map(|i| format!("{:?}", i.entries))
                .collect();
            let scopes = inventories
                .iter_mut()
                .zip(names)
                .map(|(inventory, name)| NamespaceScope {
                    inventory,
                    parent: &f.parent,
                    name: OsStr::new(name),
                })
                .collect();
            let plan = plan_batch_namespace_move(
                scopes,
                path(0, b"one/item"),
                path(1, b"two/new"),
                256,
                &cancel,
            )
            .unwrap();
            fs::rename(f.base.join("a/one/item"), f.base.join("b/two/new")).unwrap();
            match failure {
                "content" => fs::write(f.base.join("c/unrelated"), b"evil!").unwrap(),
                "root" => {
                    fs::rename(f.base.join("c"), f.base.join("kept")).unwrap();
                    fs::create_dir(f.base.join("c")).unwrap();
                }
                "cancel" => cancel.store(true, Ordering::Relaxed),
                _ => (),
            }
            assert!(
                plan.reconcile(if failure == "budget" { 18 } else { 128 }, &cancel)
                    .is_err(),
                "{failure}"
            );
            for (inv, before) in inventories.iter().zip(before) {
                assert_eq!(format!("{:?}", inv.entries), before, "{failure}");
            }
        }
    }
    #[test]
    fn batch_namespace_structural_budget_boundaries_without_filesystem_claims() {
        let mut entries = BatchBudget {
            entries: MAX_BATCH_NAMESPACE_ENTRIES - 1,
            ..Default::default()
        };
        entries.charge(1, 1).unwrap();
        assert!(entries.charge(1, 1).is_err());
        let mut names = BatchBudget {
            names: MAX_BATCH_NAMESPACE_NAMES - 1,
            ..Default::default()
        };
        names.charge(3, 1).unwrap();
        assert!(names.charge(3, 1).is_err());
        let mut wire = BatchBudget {
            wire: MAX_BATCH_NAMESPACE_WIRE - 148,
            ..Default::default()
        };
        wire.charge(1, 1).unwrap();
        assert!(wire.charge(1, 1).is_err());
        assert_eq!(MAX_BATCH_NAMESPACE_SCOPES, 16386);
        assert_eq!(MAX_BATCH_NAMESPACE_ENTRIES, 49154);
    }
    #[test]
    fn namespace_file_move_updates_both_parents_then_composes_external_alias_receipt() {
        for cross in [false, true] {
            let mut f = Fixture::new(false);
            let base = f.base.clone();
            let cancel = AtomicBool::new(false);
            let control = Control::default();
            let mut outside =
                capture_for_sync(&f.parent, OsStr::new("outside"), 4, &cancel).unwrap();
            let plan = plan_namespace_move_controlled(
                f.scopes(cross),
                path(0, b"one/item"),
                path(usize::from(cross), b"two/new"),
                128,
                &control,
            )
            .unwrap();
            assert_eq!(plan.preparation_read(), if cross { 36 } else { 18 });
            fs::rename(
                base.join("a/one/item"),
                base.join(if cross { "b/two/new" } else { "a/two/new" }),
            )
            .unwrap();
            let change = plan.reconcile(64, &cancel).unwrap();
            assert_eq!(change.payload_read(), if cross { 18 } else { 9 });
            let mut aliases = vec![AliasScope {
                inventory: &mut outside,
                parent: &f.parent,
                name: OsStr::new("outside"),
            }];
            if !cross {
                aliases.push(AliasScope {
                    inventory: &mut f.b,
                    parent: &f.parent,
                    name: OsStr::new("b"),
                });
            }
            assert_eq!(
                change
                    .reconcile_aliases(&mut aliases, 13, &cancel)
                    .unwrap()
                    .occurrences,
                if cross { 1 } else { 2 }
            );
            drop(aliases);
            f.a.verify_for_sync(&f.parent, OsStr::new("a"), 64, &cancel)
                .unwrap();
            f.b.verify_for_sync(&f.parent, OsStr::new("b"), 64, &cancel)
                .unwrap();
            let (inv, root) = if cross {
                (&f.b, OsStr::new("b"))
            } else {
                (&f.a, OsStr::new("a"))
            };
            for suffix in [b"two".as_slice(), b"two/new"] {
                let name = full(root, suffix);
                let index = inv
                    .entries
                    .binary_search_by(|e| e.relative.cmp(&name))
                    .unwrap();
                let values = inv
                    .metadata_values_for_sync(&f.parent, root, index, &cancel)
                    .unwrap();
                assert_eq!(values.evidence_digest(), inv.entries[index].metadata_digest);
            }
        }
    }
    #[test]
    fn namespace_same_parent_file_and_directory_renames_keep_nlink_exact() {
        for directory in [false, true] {
            let mut f = Fixture::new(directory);
            let base = f.base.clone();
            let cancel = AtomicBool::new(false);
            let index =
                f.a.entries
                    .binary_search_by(|e| e.relative.as_slice().cmp(b"a/one"))
                    .unwrap();
            let links = f.a.entries[index].revision.links;
            let plan = plan_namespace_move(
                f.scopes(false),
                path(0, b"one/item"),
                path(0, b"one/new"),
                128,
                &cancel,
            )
            .unwrap();
            fs::rename(base.join("a/one/item"), base.join("a/one/new")).unwrap();
            plan.reconcile(64, &cancel).unwrap();
            let index =
                f.a.entries
                    .binary_search_by(|e| e.relative.as_slice().cmp(b"a/one"))
                    .unwrap();
            assert_eq!(f.a.entries[index].revision.links, links);
            f.a.verify_for_sync(&f.parent, OsStr::new("a"), 64, &cancel)
                .unwrap();
        }
    }
    #[test]
    fn namespace_directory_transfer_rebases_subtree_and_keeps_child_revisions_exact() {
        use std::os::unix::ffi::OsStringExt;
        let mut f = Fixture::new(true);
        let base = f.base.clone();
        let cancel = AtomicBool::new(false);
        let children: Vec<_> =
            f.a.entries
                .iter()
                .filter(|e| e.relative.starts_with(b"a/one/item/"))
                .map(|e| {
                    (
                        e.relative[b"a/one/item".len()..].to_vec(),
                        e.revision,
                        e.content_digest,
                        e.metadata_digest,
                    )
                })
                .collect();
        let raw = std::ffi::OsString::from_vec(b"raw-\xff".to_vec());
        let mut to = b"two/".to_vec();
        to.extend_from_slice(raw.as_bytes());
        let plan = plan_namespace_move(
            f.scopes(true),
            path(0, b"one/item"),
            path(1, &to),
            128,
            &cancel,
        )
        .unwrap();
        assert_eq!(plan.preparation_read(), 28);
        fs::rename(base.join("a/one/item"), base.join("b/two").join(&raw)).unwrap();
        let change = plan.reconcile(64, &cancel).unwrap();
        assert_eq!(change.identity.kind, Kind::Directory);
        assert_eq!(change.payload_read(), 14);
        for (suffix, revision, content, metadata) in children {
            let mut full = b"b/".to_vec();
            full.extend_from_slice(&to);
            full.extend_from_slice(&suffix);
            let e = &f.b.entries[f
                .b
                .entries
                .binary_search_by(|e| e.relative.cmp(&full))
                .unwrap()];
            assert_eq!(e.revision, revision);
            assert_eq!(e.content_digest, content);
            assert_eq!(e.metadata_digest, metadata);
        }
        f.a.verify_for_sync(&f.parent, OsStr::new("a"), 64, &cancel)
            .unwrap();
        f.b.verify_for_sync(&f.parent, OsStr::new("b"), 64, &cancel)
            .unwrap();
    }
    #[test]
    fn namespace_two_owner_failures_never_publish_partial_recorded_state() {
        for case in [
            "extra",
            "missing",
            "replacement",
            "unrelated",
            "parentmode",
            "parentattrs",
            "nlink",
            "unrelatedctime",
            "root",
            "budget",
            "cancel",
        ] {
            let mut f = Fixture::new(false);
            let base = f.base.clone();
            let cancel = AtomicBool::new(false);
            let before_a = format!("{:?}", f.a.entries);
            let before_b = format!("{:?}", f.b.entries);
            let plan = plan_namespace_move(
                f.scopes(true),
                path(0, b"one/item"),
                path(1, b"two/new"),
                128,
                &cancel,
            )
            .unwrap();
            fs::rename(base.join("a/one/item"), base.join("b/two/new")).unwrap();
            match case {
                "extra" => fs::write(base.join("b/two/extra"), b"bad").unwrap(),
                "missing" => fs::remove_file(base.join("b/two/new")).unwrap(),
                "replacement" => {
                    fs::remove_file(base.join("b/two/new")).unwrap();
                    fs::write(base.join("b/two/new"), b"data").unwrap();
                }
                "unrelated" => fs::write(base.join("b/unrelated"), b"evil!").unwrap(),
                "parentmode" => {
                    fs::set_permissions(base.join("b/two"), fs::Permissions::from_mode(0o700))
                        .unwrap()
                }
                "parentattrs" => {
                    let p = c(base.join("b/two").as_os_str().as_bytes()).unwrap();
                    assert_eq!(
                        unsafe {
                            libc::setxattr(
                                p.as_ptr(),
                                c"user.files-namespace".as_ptr(),
                                b"bad".as_ptr().cast(),
                                3,
                                0,
                            )
                        },
                        0
                    );
                }
                "nlink" => fs::hard_link(base.join("b/two/new"), base.join("extra-link")).unwrap(),
                "unrelatedctime" => {
                    fs::set_permissions(base.join("b/unrelated"), fs::Permissions::from_mode(0o644))
                        .unwrap()
                }
                "root" => {
                    fs::rename(base.join("b"), base.join("kept")).unwrap();
                    fs::create_dir(base.join("b")).unwrap();
                }
                "cancel" => cancel.store(true, Ordering::Relaxed),
                _ => (),
            }
            assert!(
                plan.reconcile(if case == "budget" { 17 } else { 64 }, &cancel)
                    .is_err(),
                "{case}"
            );
            assert_eq!(format!("{:?}", f.a.entries), before_a, "{case}");
            assert_eq!(format!("{:?}", f.b.entries), before_b, "{case}");
            assert!(!f.a.sync.expect("input").namespace);
            assert!(!f.b.sync.expect("input").namespace);
        }
    }
    #[test]
    fn namespace_preflight_rejects_invalid_paths_cycles_existing_targets_and_budget() {
        for (from, to) in [
            (b"".as_slice(), b"two/new".as_slice()),
            (b"missing", b"two/new"),
            (b"one/item", b"unrelated"),
            (b"one/item", b"one/item/child"),
            (b"one/item", b"unrelated/new"),
            (b"../item", b"two/new"),
            (b"one/item", b"two//new"),
            (b"one/item", b"/absolute"),
        ] {
            let mut f = Fixture::new(true);
            let before = format!("{:?}", f.a.entries);
            let cancel = AtomicBool::new(false);
            let result =
                plan_namespace_move(f.scopes(false), path(0, from), path(0, to), 128, &cancel);
            assert!(result.is_err());
            drop(result);
            assert_eq!(format!("{:?}", f.a.entries), before);
        }
        let mut f = Fixture::new(false);
        let cancel = AtomicBool::new(false);
        assert!(
            plan_namespace_move(
                f.scopes(false),
                path(0, b"one/item"),
                path(0, b"two/new"),
                17,
                &cancel
            )
            .is_err()
        );
    }
    #[test]
    fn namespace_profile_overlap_mount_and_directory_link_models_fail_closed() {
        let mut f = Fixture::new(true);
        let cancel = AtomicBool::new(false);
        let index =
            f.a.entries
                .binary_search_by(|e| e.relative.as_slice().cmp(b"a/one"))
                .unwrap();
        let original = f.a.entries[index].revision.links;
        // Controlled record models, not proof of another filesystem's behavior.
        f.a.entries[index].revision.links = 1;
        assert_eq!(links_after(&f.a, index, -1).unwrap(), 1);
        assert_eq!(links_after(&f.a, index, 1).unwrap(), 1);
        f.a.entries[index].revision.links = 3;
        assert_eq!(links_after(&f.a, index, -1).unwrap(), 2);
        assert_eq!(links_after(&f.a, index, 1).unwrap(), 4);
        f.a.entries[index].revision.links = 2;
        assert!(links_after(&f.a, index, 1).is_err());
        f.a.entries[index].revision.links = original;
        let mut duplicate = capture_for_sync(&f.parent, OsStr::new("a"), 64, &cancel).unwrap();
        assert!(
            plan_namespace_move(
                vec![
                    NamespaceScope {
                        inventory: &mut f.a,
                        parent: &f.parent,
                        name: OsStr::new("a")
                    },
                    NamespaceScope {
                        inventory: &mut duplicate,
                        parent: &f.parent,
                        name: OsStr::new("a")
                    }
                ],
                path(0, b"one/item"),
                path(1, b"two/new"),
                128,
                &cancel
            )
            .is_err()
        );
        f.a.mounts.as_mut().unwrap()[index] ^= 1;
        assert!(
            plan_namespace_move(
                f.scopes(true),
                path(0, b"one/item"),
                path(1, b"two/new"),
                128,
                &cancel
            )
            .is_err()
        );
        f.a.mounts.as_mut().unwrap()[index] ^= 1;
        f.a.sync = None;
        assert!(
            plan_namespace_move(
                f.scopes(false),
                path(0, b"one/item"),
                path(0, b"two/new"),
                128,
                &cancel
            )
            .is_err()
        );
        let one = File::open(f.base.join("a/one/item/a-")).unwrap();
        assert!(capture_namespace(&one, OsStr::new("leaf"), 4, &cancel).is_err());
    }
    #[test]
    fn namespace_full_two_input_union_and_one_over_preserve_original_input_limits() {
        let base = crate::persistence::test_dir("sync-namespace-full");
        fs::create_dir_all(base.join("a/item")).unwrap();
        fs::create_dir(base.join("b")).unwrap();
        for i in 0..MAX_ENTRIES - 1 {
            fs::write(base.join("a/item").join(format!("s{i:05}")), b"").unwrap();
        }
        for i in 0..MAX_ENTRIES {
            fs::write(base.join("b").join(format!("n{i:05}")), b"").unwrap();
        }
        let parent = File::open(&base).unwrap();
        let cancel = AtomicBool::new(false);
        let control = Control::default();
        let mut a = capture_for_sync(&parent, OsStr::new("a"), 0, &cancel).unwrap();
        let mut b = capture_for_sync(&parent, OsStr::new("b"), 0, &cancel).unwrap();
        assert_eq!(a.entries.len(), MAX_ENTRIES + 1);
        assert_eq!(b.entries.len(), MAX_ENTRIES + 1);
        let plan = plan_namespace_move(
            vec![
                NamespaceScope {
                    inventory: &mut a,
                    parent: &parent,
                    name: OsStr::new("a"),
                },
                NamespaceScope {
                    inventory: &mut b,
                    parent: &parent,
                    name: OsStr::new("b"),
                },
            ],
            path(0, b"item"),
            path(1, b"new"),
            0,
            &cancel,
        )
        .unwrap();
        fs::rename(base.join("a/item"), base.join("b/new")).unwrap();
        plan.reconcile(0, &cancel).unwrap();
        assert_eq!(a.entries.len(), 1);
        assert_eq!(b.entries.len(), MAX_NAMESPACE_ENTRIES);
        a.verify_for_sync(&parent, OsStr::new("a"), 0, &cancel)
            .unwrap();
        b.verify_for_sync(&parent, OsStr::new("b"), 0, &cancel)
            .unwrap();
        assert!(capture_for_sync(&parent, OsStr::new("b"), 0, &cancel).is_err());
        let reopened = capture_namespace_controlled(&parent, OsStr::new("b"), 0, &control).unwrap();
        assert_eq!(reopened.entries, b.entries);
        fs::write(base.join("a/extra"), b"").unwrap();
        a = capture_namespace(&parent, OsStr::new("a"), 0, &cancel).unwrap();
        assert!(
            plan_namespace_move(
                vec![
                    NamespaceScope {
                        inventory: &mut a,
                        parent: &parent,
                        name: OsStr::new("a")
                    },
                    NamespaceScope {
                        inventory: &mut b,
                        parent: &parent,
                        name: OsStr::new("b")
                    }
                ],
                path(0, b"extra"),
                path(1, b"overflow"),
                0,
                &cancel
            )
            .is_err()
        );
        assert!(base.join("a/extra").exists());
        assert_eq!(b.entries.len(), MAX_NAMESPACE_ENTRIES);
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn namespace_resulting_name_and_depth_budgets_are_checked_before_rename() {
        let base = crate::persistence::test_dir("sync-namespace-names");
        fs::create_dir(base.join("a/item"))
            .or_else(|_| fs::create_dir_all(base.join("a/item")))
            .unwrap();
        for i in 0..MAX_ENTRIES - 1 {
            fs::write(base.join("a/item").join(format!("n{i:05}")), b"").unwrap();
        }
        let parent = File::open(&base).unwrap();
        let cancel = AtomicBool::new(false);
        let mut inv = capture_for_sync(&parent, OsStr::new("a"), 0, &cancel).unwrap();
        let long = vec![b'x'; 255];
        assert!(
            plan_namespace_move(
                vec![NamespaceScope {
                    inventory: &mut inv,
                    parent: &parent,
                    name: OsStr::new("a")
                }],
                path(0, b"item"),
                path(0, &long),
                0,
                &cancel
            )
            .is_err()
        );
        assert!(base.join("a/item").exists());
        fs::remove_dir_all(base).unwrap();
        let base = crate::persistence::test_dir("sync-namespace-depth");
        fs::create_dir_all(base.join("a/item")).unwrap();
        fs::write(base.join("a/item/leaf"), b"").unwrap();
        let mut relative = Vec::new();
        let mut p = base.join("a");
        for i in 0..crate::directory_compare::MAX_DEPTH {
            if !relative.is_empty() {
                relative.push(b'/');
            }
            let n = format!("d{i}");
            relative.extend_from_slice(n.as_bytes());
            p.push(n);
            fs::create_dir(&p).unwrap();
        }
        relative.extend_from_slice(b"/new");
        let parent = File::open(&base).unwrap();
        let mut inv = capture_for_sync(&parent, OsStr::new("a"), 0, &cancel).unwrap();
        assert!(
            plan_namespace_move(
                vec![NamespaceScope {
                    inventory: &mut inv,
                    parent: &parent,
                    name: OsStr::new("a")
                }],
                path(0, b"item"),
                path(0, &relative),
                0,
                &cancel
            )
            .is_err()
        );
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn namespace_control_pause_resume_and_cancel_keep_recorded_state_atomic() {
        use std::{sync::mpsc, time::Duration};
        for cancel in [false, true] {
            let mut f = Fixture::new(false);
            let base = f.base.clone();
            let before = format!("{:?}", f.a.entries);
            let control = Control::default();
            control.pause();
            let worker_control = control.clone();
            let (entered_tx, entered_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            let worker = std::thread::spawn(move || {
                let flag = AtomicBool::new(false);
                let plan = plan_namespace_move(
                    f.scopes(false),
                    path(0, b"one/item"),
                    path(0, b"two/new"),
                    128,
                    &flag,
                )
                .unwrap();
                fs::rename(base.join("a/one/item"), base.join("a/two/new")).unwrap();
                entered_tx.send(()).unwrap();
                let result = plan.reconcile_controlled(64, &worker_control);
                done_tx
                    .send((result, format!("{:?}", f.a.entries)))
                    .unwrap();
            });
            entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            assert!(done_rx.recv_timeout(Duration::from_millis(30)).is_err());
            if cancel {
                control.cancel();
            } else {
                control.resume();
            }
            let (result, after) = done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            worker.join().unwrap();
            if cancel {
                assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
                assert_eq!(before, after);
            } else {
                assert_eq!(result.unwrap().payload_read(), 9);
            }
        }
    }
}
