//! Pure raw-path grouping/budget first, then shared pinned containers.
use super::*;
use crate::directory_sync_admission::StagingAdmission;
use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};
pub const MAX_BATCH_METADATA_BYTES: usize = 16 * 1024 * 1024;
pub(super) type PlacementSlot = (Arc<Directory>, OsString, [u8; 32]);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkspaceBatchReport {
    unique: usize,
    bytes: usize,
    fds: usize,
}
impl WorkspaceBatchReport {
    /// Pure inspection of layout data; no filesystem access or approval.
    pub fn inspect(physical: &Physical<'_, '_>, parents: &[&Path]) -> io::Result<Self> {
        let a = physical.staging_admission()?;
        Ok(Layout::describe(&a, parents)?.report)
    }
    pub fn distinct_paths(&self) -> usize {
        self.unique
    }
    /// Requested retained payload/headers only, not resident/allocator memory.
    pub fn retained_metadata_bytes(&self) -> usize {
        self.bytes
    }
    pub fn retained_fd_slots(&self) -> usize {
        self.fds
    }
}
fn charge(total: &mut usize, extra: usize, limit: usize) -> io::Result<()> {
    *total = total
        .checked_add(extra)
        .filter(|&n| n <= limit)
        .ok_or_else(|| bad("Metadatos retenidos de placements exceden presupuesto"))?;
    Ok(())
}
pub(super) struct Layout<'p> {
    pub report: WorkspaceBatchReport,
    unique: Vec<&'p Path>,
    groups: Vec<usize>,
}
impl<'p> Layout<'p> {
    fn group(parents: &[&'p Path], count: usize, limit: usize) -> io::Result<Self> {
        if parents.len() != count || count > crate::directory_sync_physical::MAX_TRANSACTIONS {
            return Err(bad("Lote de placements incompleto/excedido"));
        }
        let mut bytes = 0;
        // Exact reservations for transaction slots, returned receipt headers,
        // one canonical 46-byte basename, group index and borrowed unique-path
        // slot per transaction. The parent-vector Arc slot is charged per unique.
        let per = std::mem::size_of::<PlacementSlot>()
            + std::mem::size_of::<CreatedWorkspace>()
            + 46
            + std::mem::size_of::<usize>()
            + std::mem::size_of::<&Path>();
        charge(
            &mut bytes,
            count
                .checked_mul(per)
                .ok_or_else(|| bad("Metadatos de lote excedidos"))?,
            limit,
        )?;
        let mut seen = BTreeMap::<&[u8], usize>::new();
        let mut unique = Vec::with_capacity(count);
        let mut groups = Vec::with_capacity(count);
        for path in parents {
            let raw = path.as_os_str().as_bytes();
            if let Some(&index) = seen.get(raw) {
                groups.push(index);
                continue;
            }
            let (components, names) = Directory::path_shape(path)?;
            let fixed = std::mem::size_of::<Directory>()
                + 2 * std::mem::size_of::<usize>()
                + std::mem::size_of::<Arc<Directory>>(); // Arc strong/weak header.
            let requested = fixed
                + raw.len()
                + names
                + components * std::mem::size_of::<OsString>()
                + (components + 1) * std::mem::size_of::<Node>();
            charge(&mut bytes, requested, limit)?;
            let index = unique.len();
            seen.insert(raw, index);
            unique.push(*path);
            groups.push(index);
        }
        Ok(Self {
            report: WorkspaceBatchReport {
                unique: unique.len(),
                bytes,
                fds: 0,
            },
            unique,
            groups,
        })
    }
    pub fn describe(admission: &StagingAdmission, parents: &[&'p Path]) -> io::Result<Self> {
        let mut layout = Self::group(
            parents,
            admission.workspaces().len(),
            MAX_BATCH_METADATA_BYTES,
        )?;
        layout.report.fds = admission.retained_fd_slots_for_containers(layout.unique.len())?;
        Ok(layout)
    }
    pub fn open(
        &self,
        physical: &Physical<'_, '_>,
        store: &Directory,
        checkpoint: &impl Fn() -> io::Result<()>,
    ) -> io::Result<Vec<Arc<Directory>>> {
        let excluded = Exclusions::new(physical)?;
        let mut dirs = Vec::with_capacity(self.unique.len());
        for path in &self.unique {
            checkpoint()?;
            let directory = Directory::open(path)?;
            excluded.check(&directory, store)?;
            dirs.push(Arc::new(directory));
        }
        Ok(dirs)
    }
    pub fn parent(&self, dirs: &[Arc<Directory>], transaction: usize) -> Arc<Directory> {
        Arc::clone(&dirs[self.groups[transaction]])
    }
}
fn overlaps_store(container: &[Node], store: &[Node]) -> bool {
    let identity = |n: &Node| (n.device, n.inode);
    container
        .iter()
        .any(|n| identity(n) == identity(store.last().expect("store")))
        || store
            .iter()
            .any(|n| identity(n) == identity(container.last().expect("container")))
}
struct Exclusions {
    forbidden: HashSet<(u64, u64)>,
    roots: HashSet<(u64, u64)>,
}
impl Exclusions {
    fn new(physical: &Physical<'_, '_>) -> io::Result<Self> {
        let mut forbidden = HashSet::new();
        let mut roots = HashSet::new();
        for root in &physical.plan().record().view()?.roots {
            roots.insert((
                root.entries[0].identity.device,
                root.entries[0].identity.inode,
            ));
            for e in &root.entries {
                if e.identity.kind == Kind::Directory {
                    forbidden.insert((e.identity.device, e.identity.inode));
                }
            }
            for a in &root.ancestors {
                forbidden.insert((a.device, a.inode));
            }
        }
        Ok(Self { forbidden, roots })
    }
    fn check(&self, d: &Directory, store: &Directory) -> io::Result<()> {
        let mine = d.chain.last().expect("parent");
        if self.forbidden.contains(&(mine.device, mine.inode))
            || d.chain
                .iter()
                .any(|n| self.roots.contains(&(n.device, n.inode)))
        {
            return Err(bad(
                "Container y carpetas sincronizadas deben ser disjuntos",
            ));
        }
        if overlaps_store(&d.chain, &store.chain) {
            return Err(bad("Container solapa almacén"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batch_store_exclusion_rejects_underlying_alias_ids_across_mounts() {
        // Predicate coverage only; these are not actual bind-mount observations.
        let node = |device, inode, mount| Node {
            device,
            inode,
            mount,
        };
        let store = [node(1, 1, 10), node(1, 2, 10)];
        assert!(overlaps_store(
            &[node(1, 1, 20), node(1, 2, 20), node(1, 3, 20)],
            &store
        ));
        assert!(overlaps_store(&[node(1, 1, 20)], &store));
        assert!(!overlaps_store(&[node(1, 1, 20), node(1, 4, 20)], &store));
        assert!(!overlaps_store(&[node(2, 2, 20)], &store));
    }
    fn deep_path(index: usize) -> PathBuf {
        let mut parts = vec![vec![b'a'; 31]; 128];
        parts[127][..8].copy_from_slice(format!("{index:08x}").as_bytes());
        PathBuf::from(OsString::from_vec(
            [b"/".as_slice(), &parts.join(&b'/')].concat(),
        ))
    }
    #[test]
    fn batch_layout_full_transaction_count_shares_one_maximum_depth_path() {
        let path = deep_path(0);
        assert_eq!(path.as_os_str().as_bytes().len(), 4096);
        let parents = vec![path.as_path(); crate::directory_sync_physical::MAX_TRANSACTIONS];
        let layout = Layout::group(&parents, parents.len(), MAX_BATCH_METADATA_BYTES).unwrap();
        assert_eq!(layout.unique.len(), 1);
        assert!(layout.groups.iter().all(|&g| g == 0));
        assert!(layout.report.bytes < MAX_BATCH_METADATA_BYTES);
        assert!(Layout::group(&parents, parents.len() - 1, MAX_BATCH_METADATA_BYTES).is_err());
    }
    #[test]
    fn batch_layout_exact_budget_and_raw_syntax_are_checked_before_pinning() {
        let path = PathBuf::from(OsString::from_vec(b"/raw/\xff\n".to_vec()));
        let parents = [path.as_path(), path.as_path(), Path::new("/raw/other")];
        let layout = Layout::group(&parents, 3, MAX_BATCH_METADATA_BYTES).unwrap();
        assert_eq!(layout.groups, vec![0, 0, 1]);
        let bytes = layout.report.bytes;
        assert!(Layout::group(&parents, 3, bytes).is_ok());
        assert!(Layout::group(&parents, 3, bytes - 1).is_err());
        for invalid in [
            "/raw/./other",
            "/raw//other",
            "/raw/../other",
            "relative",
            "/raw/other/",
        ] {
            assert!(
                Layout::group(
                    &[Path::new("/raw/other"), Path::new(invalid)],
                    2,
                    MAX_BATCH_METADATA_BYTES
                )
                .is_err()
            );
        }
    }
    #[test]
    fn batch_layout_distinct_deep_paths_exceed_metadata_budget_without_opening_paths() {
        let paths: Vec<_> = (0..2048).map(deep_path).collect();
        let parents: Vec<_> = paths.iter().map(PathBuf::as_path).collect();
        assert!(Layout::group(&parents, parents.len(), MAX_BATCH_METADATA_BYTES).is_err());
        assert!(Layout::group(&[], 0, 0).is_ok());
        assert!(
            Layout::group(
                &[],
                crate::directory_sync_physical::MAX_TRANSACTIONS + 1,
                MAX_BATCH_METADATA_BYTES
            )
            .is_err()
        );
    }
}
