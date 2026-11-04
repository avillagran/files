//! Pure whole-batch capture geometry. Reports are data, never approval or FD reservations.
use crate::{
    directory_sync_physical::Physical,
    move_manifest::{Kind, MAX_PATH, MAX_WIRE},
};
use std::{collections::BTreeMap, ffi::OsString, io};
pub const MAX_SCOPE_ENTRIES: usize = 2 * crate::move_manifest::MAX_ENTRIES + 1;
pub const MAX_SCOPE_NAMES: usize = 2 * crate::directory_compare::MAX_NAMES;
pub const MAX_SCOPE_WIRE: usize = MAX_WIRE * 4 - 16384;
pub const MAX_SCOPE_DEPTH: usize = crate::directory_compare::MAX_DEPTH + 1;
pub const MAX_SCOPE_PENDING: usize = 4 * 1024 * 1024;
const ROOT_PREFIX: usize = 255;
fn bad(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopeShape {
    pub entries: usize,
    pub retained_names: usize,
    pub wire_upper: usize,
    pub max_path: usize,
    pub max_depth: usize,
    pub peak_pending_names_upper: usize,
}
#[derive(Debug)]
pub struct WorkspaceShape {
    pub transaction: usize,
    pub staged: ScopeShape,
    pub preserved_union: ScopeShape,
}
#[derive(Debug)]
pub struct StagingAdmission {
    seal: [u8; 32],
    workspaces: Vec<WorkspaceShape>,
    input_union: ScopeShape,
    receipt_fds: usize,
    retained_fds: usize,
}
impl StagingAdmission {
    pub fn physical_seal(&self) -> [u8; 32] {
        self.seal
    }
    pub fn workspaces(&self) -> &[WorkspaceShape] {
        &self.workspaces
    }
    /// Directory-promoted union of both inputs; bounds every target intermediate.
    pub fn input_union(&self) -> ScopeShape {
        self.input_union
    }
    pub fn peak_receipt_fds(&self) -> usize {
        self.receipt_fds
    }
    /// Defined staging-retention policy only; process/transient headroom is separate.
    pub fn retained_fd_slots(&self) -> usize {
        self.retained_fds
    }
    /// Same retention policy with one shared container pin per distinct raw path.
    /// Data only: not OS availability, a reservation or caller lifetime enforcement.
    pub fn retained_fd_slots_for_containers(&self, unique: usize) -> io::Result<usize> {
        let total = self.workspaces.len();
        if unique > total || (total != 0 && unique == 0) {
            return Err(bad("Conteo de containers inválido"));
        }
        self.retained_fds
            .checked_sub(total - unique)
            .ok_or_else(|| bad("Forecast FD incoherente"))
    }
    pub fn require_fd_slots(&self, available_for_staging: usize) -> io::Result<()> {
        if available_for_staging < self.retained_fds {
            Err(bad("Slots FD retenidos insuficientes para staging"))
        } else {
            Ok(())
        }
    }
}
struct Item {
    kind: Kind,
    children: usize,
    child_names: usize,
    peak: usize,
}
struct Shape {
    rows: BTreeMap<Vec<u8>, Item>,
    stats: ScopeShape,
}
impl Shape {
    fn new() -> Self {
        let mut rows = BTreeMap::new();
        rows.insert(
            Vec::new(),
            Item {
                kind: Kind::Directory,
                children: 0,
                child_names: 0,
                peak: 0,
            },
        );
        Self {
            rows,
            stats: ScopeShape {
                entries: 1,
                retained_names: 0,
                wire_upper: 147 + ROOT_PREFIX,
                max_path: 0,
                max_depth: 0,
                peak_pending_names_upper: 0,
            },
        }
    }
    /// Charge every prospective path before allocating its joined raw bytes.
    fn add(&mut self, prefix: &[u8], suffix: &[u8], kind: Kind, merge: bool) -> io::Result<()> {
        let separator = usize::from(!prefix.is_empty() && !suffix.is_empty());
        let len = prefix
            .len()
            .checked_add(separator)
            .and_then(|v| v.checked_add(suffix.len()))
            .ok_or_else(|| bad("Ruta staging excedida"))?;
        if len == 0 {
            return Ok(());
        }
        let depth = 1 + prefix.iter().chain(suffix).filter(|&&b| b == b'/').count() + separator;
        if len > MAX_PATH || depth > MAX_SCOPE_DEPTH {
            return Err(bad("Ruta/profundidad staging excede captura"));
        }
        if merge {
            if !prefix.is_empty() {
                return Err(bad("Envelope requiere nombres completos"));
            }
            if let Some(old) = self.rows.get_mut(suffix) {
                if kind == Kind::Directory {
                    old.kind = kind;
                }
                return Ok(());
            }
        }
        self.charge(len, depth)?;
        let mut path = Vec::with_capacity(len);
        path.extend_from_slice(prefix);
        if separator != 0 {
            path.push(b'/');
        }
        path.extend_from_slice(suffix);
        if self.rows.contains_key(&path) {
            return Err(bad("Nombre staging duplicado"));
        }
        self.rows.insert(
            path,
            Item {
                kind,
                children: 0,
                child_names: 0,
                peak: 0,
            },
        );
        Ok(())
    }
    fn charge(&mut self, len: usize, depth: usize) -> io::Result<()> {
        let entries = self
            .stats
            .entries
            .checked_add(1)
            .filter(|&n| n <= MAX_SCOPE_ENTRIES)
            .ok_or_else(|| bad("Conteo staging excede captura"))?;
        let names = self
            .stats
            .retained_names
            .checked_add(len)
            .filter(|&n| n <= MAX_SCOPE_NAMES)
            .ok_or_else(|| bad("Nombres staging exceden captura"))?;
        let wire = self
            .stats
            .wire_upper
            .checked_add(148 + ROOT_PREFIX + len)
            .filter(|&n| n <= MAX_SCOPE_WIRE)
            .ok_or_else(|| bad("Wire staging excede captura"))?;
        self.stats.entries = entries;
        self.stats.retained_names = names;
        self.stats.wire_upper = wire;
        self.stats.max_path = self.stats.max_path.max(len);
        self.stats.max_depth = self.stats.max_depth.max(depth);
        Ok(())
    }
    fn finish(mut self) -> io::Result<ScopeShape> {
        // Reverse raw order visits every child before its parent. Track each
        // directory's child capacity against Walker's doubling/min32 rule.
        let keys: Vec<_> = self.rows.keys().cloned().collect();
        for path in keys.into_iter().rev() {
            let item = self.rows.get(&path).expect("row");
            if item.kind != Kind::Directory && item.children != 0 {
                return Err(bad("Padre staging no es directorio"));
            }
            // Upper bound for every doubling/min32 reservation sequence,
            // independent of allocator over-allocation. This is charged-name
            // work, not an assertion about resident memory.
            let slots = if item.children == 0 {
                0
            } else {
                2 * item.children + 32
            };
            let own = item
                .child_names
                .checked_add(slots * std::mem::size_of::<OsString>())
                .ok_or_else(|| bad("Trabajo nombres staging excedido"))?;
            let peak = own
                .checked_add(item.peak)
                .filter(|&n| n <= MAX_SCOPE_PENDING)
                .ok_or_else(|| bad("Trabajo pendiente staging excede captura"))?;
            if path.is_empty() {
                self.stats.peak_pending_names_upper = peak;
                break;
            }
            let at = path.iter().rposition(|&b| b == b'/');
            let (parent, name) = at.map_or((b"".as_slice(), path.as_slice()), |i| {
                (&path[..i], &path[i + 1..])
            });
            let p = self
                .rows
                .get_mut(parent)
                .ok_or_else(|| bad("Padre staging ausente"))?;
            p.children += 1;
            p.child_names += name.len();
            p.peak = p.peak.max(peak);
        }
        Ok(self.stats)
    }
}
fn suffix<'a>(path: &'a [u8], root: &[u8]) -> io::Result<&'a [u8]> {
    if path == root {
        Ok(b"")
    } else {
        path.strip_prefix(root)
            .and_then(|p| p.strip_prefix(b"/"))
            .ok_or_else(|| bad("Subárbol staging incoherente"))
    }
}
impl Physical<'_, '_> {
    /// Pure check of every new+old workspace union and all possible target names.
    /// No filesystem access, human approval, reservation or freshness is supplied.
    pub fn staging_admission(&self) -> io::Result<StagingAdmission> {
        let view = self.plan().record().view()?;
        let mut workspaces = Vec::with_capacity(self.transactions().len());
        for (index, t) in self.transactions().iter().enumerate() {
            let target = self
                .plan()
                .name(t.row())
                .ok_or_else(|| bad("Fila staging ausente"))?;
            let mut shape = Shape::new();
            let source_root = t.nodes().next().map(|i| self.nodes()[i].source());
            if let Some(root) = source_root {
                let prefix = view.roots[root.side].entries[root.index].relative;
                if prefix != target {
                    return Err(bad("Mapeo destino no cubierto por envelope"));
                }
                for i in t.nodes() {
                    let l = self.nodes()[i].source();
                    let e = &view.roots[l.side].entries[l.index];
                    shape.add(b"new", suffix(e.relative, prefix)?, e.identity.kind, false)?;
                }
            }
            // Finalize a bounded clone for the staging-only diagnostic; union is
            // the admission bound for preserve/publish and reverse transitions.
            let staged = Shape {
                rows: shape
                    .rows
                    .iter()
                    .map(|(p, v)| {
                        (
                            p.clone(),
                            Item {
                                kind: v.kind,
                                children: 0,
                                child_names: 0,
                                peak: 0,
                            },
                        )
                    })
                    .collect(),
                stats: shape.stats,
            }
            .finish()?;
            if let Some(root) = t.old() {
                let prefix = view.roots[root.side].entries[root.index].relative;
                if prefix != target {
                    return Err(bad("Backup fuera del envelope"));
                }
                for l in &self.preserved_entries()[t.backup_entries()] {
                    let e = &view.roots[l.side].entries[l.index];
                    shape.add(b"old", suffix(e.relative, prefix)?, e.identity.kind, false)?;
                }
            }
            workspaces.push(WorkspaceShape {
                transaction: index,
                staged,
                preserved_union: shape.finish()?,
            });
        }
        let mut union = Shape::new();
        for root in &view.roots {
            for e in root.entries.iter().skip(1) {
                union.add(b"", e.relative, e.identity.kind, true)?;
            }
        }
        let mut live = 0usize;
        let mut peak = 0usize;
        for (i, n) in self.nodes().iter().enumerate() {
            peak = peak.max(live + 1);
            if let Some(g) = n.cohort() {
                let members = self.cohorts()[g].members();
                let first = members[0];
                let last = *members.last().expect("cohort");
                if first == i && last > i {
                    live += 1;
                }
                if last == i && first < i {
                    live -= 1;
                }
            }
        }
        let roots = self.transactions().len();
        let creation = 2 * roots + peak;
        let metadata = if self.stage_metadata_order()?.is_empty() {
            2 * roots
        } else {
            3 * roots
        };
        Ok(StagingAdmission {
            seal: self.seal()?,
            workspaces,
            input_union: union.finish()?,
            receipt_fds: peak,
            retained_fds: creation.max(metadata),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_entry_envelope_and_first_excess() {
        let mut shape = Shape::new();
        for i in 1..MAX_SCOPE_ENTRIES {
            shape
                .add(b"", format!("{i:05}").as_bytes(), Kind::File, false)
                .unwrap();
        }
        assert!(shape.add(b"", b"overflow", Kind::File, false).is_err());
        let s = shape.finish().unwrap();
        assert_eq!(s.entries, MAX_SCOPE_ENTRIES);
        assert!(s.peak_pending_names_upper < MAX_SCOPE_PENDING);
    }
    #[test]
    fn exact_names_budget_is_charged_before_join_allocation() {
        let mut shape = Shape::new();
        for _ in 0..512 {
            shape.charge(4096, 17).unwrap();
        }
        assert_eq!(shape.stats.retained_names, MAX_SCOPE_NAMES);
        let before = shape.stats;
        assert!(shape.charge(1, 1).is_err());
        assert_eq!(shape.stats, before);
    }
    #[test]
    fn raw_paths_remapping_and_depth_limits() {
        let mut shape = Shape::new();
        let path = vec![0xff; MAX_PATH];
        shape.add(b"", &path, Kind::File, false).unwrap();
        assert!(
            Shape::new()
                .add(b"new", &path[2..], Kind::File, false)
                .is_err()
        );
        let deep = vec![b"a".as_slice(); MAX_SCOPE_DEPTH].join(&b'/');
        Shape::new().add(b"", &deep, Kind::File, false).unwrap();
        assert!(Shape::new().add(b"new", &deep, Kind::File, false).is_err());
    }
    #[test]
    fn union_promotes_conflicting_leaf_to_directory_in_both_orders() {
        for reverse in [false, true] {
            let mut shape = Shape::new();
            let kinds = if reverse {
                [Kind::Directory, Kind::File]
            } else {
                [Kind::File, Kind::Directory]
            };
            for kind in kinds {
                shape.add(b"", b"x", kind, true).unwrap();
            }
            shape.add(b"", b"x/raw\xff", Kind::Symlink, true).unwrap();
            let s = shape.finish().unwrap();
            assert_eq!(s.entries, 3);
            assert_eq!(s.retained_names, 7);
            assert_eq!(s.max_depth, 2);
        }
    }
    #[test]
    fn malformed_parent_graphs_and_duplicates_reject() {
        let mut missing = Shape::new();
        missing.add(b"", b"x/y", Kind::File, false).unwrap();
        assert!(missing.finish().is_err());
        let mut leaf = Shape::new();
        leaf.add(b"", b"x", Kind::File, false).unwrap();
        leaf.add(b"", b"x/y", Kind::File, false).unwrap();
        assert!(leaf.finish().is_err());
        let mut duplicate = Shape::new();
        duplicate.add(b"", b"x", Kind::Directory, false).unwrap();
        assert!(duplicate.add(b"", b"x", Kind::Directory, false).is_err());
    }
    #[test]
    fn pending_work_accounts_for_ancestor_child_lists() {
        let mut shape = Shape::new();
        shape.add(b"", b"new", Kind::Directory, false).unwrap();
        shape.add(b"new", b"a", Kind::File, false).unwrap();
        shape.add(b"new", b"b", Kind::File, false).unwrap();
        let s = shape.finish().unwrap();
        assert_eq!(
            s.peak_pending_names_upper,
            5 + 70 * std::mem::size_of::<OsString>()
        );
        // Every active ancestor contributes disjoint child entries and raw names.
        // Even the global conservative maximum remains within the runtime quota.
        assert!(
            MAX_SCOPE_NAMES
                + (2 * MAX_SCOPE_ENTRIES + 32 * (MAX_SCOPE_DEPTH + 1))
                    * std::mem::size_of::<OsString>()
                < MAX_SCOPE_PENDING
        );
    }
}
