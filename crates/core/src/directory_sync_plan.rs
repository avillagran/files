//! Logical sync effects/decisions derived from canonical evidence. No executor,
//! operator approval or journal barrier. Decoded plans are review data only.
use crate::{
    directory_sync::{Conflict, Mode, Side},
    directory_sync_record::{EntryView, Record, View},
    move_manifest::Kind,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io,
    sync::atomic::{AtomicU64, Ordering},
};
const MAGIC: &[u8; 8] = b"FLSPLN\0\x01";
const DOMAIN: &[u8] = b"Files:sync-action-plan:v1\0";
pub const MAX_LINES: usize = 2 * crate::directory_compare::MAX_ITEMS + 1;
pub const MAX_WIRE: usize = 8 + 32 + 3 + 4 + MAX_LINES + 32;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Metadata {
    Ignore,
    Basic,
    ExactObserved,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hardlinks {
    /// Each selected materialization receives an independent new inode.
    IndependentNames,
    /// Preserve relationships among selected materializations of one source
    /// inode. Skipped/retained/unchanged names are not implicitly rewritten;
    /// complete topology reconciliation needs its own explicit review.
    PreserveInternal,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    pub metadata: Metadata,
    pub root_metadata: bool,
    pub hardlinks: Hardlinks,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            metadata: Metadata::Basic,
            root_metadata: false,
            hardlinks: Hardlinks::PreserveInternal,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proposal {
    Unchanged,
    Retain,
    MergeDirectory,
    CopyFrom(Side),
    Review(Conflict),
    CoveredBy(usize),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    None,
    Pending,
    Include,
    Skip,
    Take(Side),
    Retire,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    MergeDirectory,
    CoveredBy(usize),
    CopyFrom(Side),
    PreserveAndReplaceFrom(Side),
    PreserveAndSetMetadataFrom(Side),
    PreserveAndRetire,
}
#[derive(Debug)]
pub struct Line {
    locations: [Option<usize>; 2],
    proposal: Proposal,
    decision: Decision,
    shared_directory: bool,
}
impl Line {
    pub fn locations(&self) -> [Option<usize>; 2] {
        self.locations
    }
    pub fn proposal(&self) -> Proposal {
        self.proposal
    }
    pub fn decision(&self) -> Decision {
        self.decision
    }
    pub fn shared_directory(&self) -> bool {
        self.shared_directory
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Receipt {
    instance: u64,
    revision: u64,
    row: usize,
}
impl Receipt {
    pub fn row(self) -> usize {
        self.row
    }
}
pub struct Plan<'a> {
    record: &'a Record,
    view: View<'a>,
    options: Options,
    lines: Vec<Line>,
    instance: u64,
    revision: u64,
}
fn bad() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "Plan o decisión de sincronización inválido/obsoleto",
    )
}
fn seal(bytes: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(DOMAIN);
    h.update(bytes);
    h.finalize().into()
}
fn side_allowed(mode: Mode, side: Side) -> bool {
    match mode {
        Mode::Update(s) | Mode::Mirror(s) => s == side,
        Mode::Merge => true,
    }
}
fn metadata_diff(a: &EntryView<'_>, b: &EntryView<'_>, policy: Metadata) -> bool {
    match policy {
        Metadata::Ignore => false,
        Metadata::Basic => {
            a.revision.mode != b.revision.mode
                || a.identity.kind != Kind::Directory
                    && (a.revision.mtime_seconds, a.revision.mtime_nanos)
                        != (b.revision.mtime_seconds, b.revision.mtime_nanos)
        }
        Metadata::ExactObserved => a.metadata != b.metadata,
    }
}
fn classify(
    a: Option<&EntryView<'_>>,
    b: Option<&EntryView<'_>>,
    mode: Mode,
    policy: Metadata,
) -> Proposal {
    match (a, b) {
        (Some(a), Some(b)) => {
            if a.identity.kind != b.identity.kind {
                return Proposal::Review(Conflict::Type);
            }
            if a.identity.kind != Kind::Directory && a.content != b.content {
                return Proposal::Review(if a.identity.kind == Kind::File {
                    Conflict::Content
                } else {
                    Conflict::Link
                });
            }
            if metadata_diff(a, b, policy) {
                return Proposal::Review(Conflict::Metadata);
            }
            if a.identity.kind == Kind::Directory {
                Proposal::MergeDirectory
            } else {
                Proposal::Unchanged
            }
        }
        (a, b) => {
            let present = if a.is_some() { Side::Left } else { Side::Right };
            debug_assert!(a.is_some() || b.is_some());
            match mode {
                Mode::Merge => Proposal::CopyFrom(present),
                Mode::Update(s) | Mode::Mirror(s) if s == present => Proposal::CopyFrom(present),
                Mode::Update(_) => Proposal::Retain,
                Mode::Mirror(_) => Proposal::Review(Conflict::DestinationOnly),
            }
        }
    }
}
fn allowed(line: &Line, decision: Decision, mode: Mode) -> bool {
    match line.proposal {
        Proposal::CopyFrom(_) => matches!(decision, Decision::Include | Decision::Skip),
        Proposal::Review(c) => match decision {
            Decision::Pending | Decision::Skip => true,
            Decision::Retire => c == Conflict::DestinationOnly,
            Decision::Take(s) => c != Conflict::DestinationOnly && side_allowed(mode, s),
            _ => false,
        },
        _ => decision == Decision::None,
    }
}
fn decision_byte(d: Decision) -> u8 {
    match d {
        Decision::None => 0,
        Decision::Pending => 1,
        Decision::Include => 2,
        Decision::Skip => 3,
        Decision::Take(Side::Left) => 4,
        Decision::Take(Side::Right) => 5,
        Decision::Retire => 6,
    }
}
fn decision(byte: u8) -> io::Result<Decision> {
    Ok(match byte {
        0 => Decision::None,
        1 => Decision::Pending,
        2 => Decision::Include,
        3 => Decision::Skip,
        4 => Decision::Take(Side::Left),
        5 => Decision::Take(Side::Right),
        6 => Decision::Retire,
        _ => return Err(bad()),
    })
}
impl<'a> Plan<'a> {
    pub fn new(record: &'a Record, options: Options) -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let mut instance = NEXT.load(Ordering::Relaxed);
        loop {
            let next = instance.checked_add(1).ok_or_else(bad)?;
            match NEXT.compare_exchange_weak(instance, next, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(current) => instance = current,
            }
        }
        let view = record.view()?;
        let mut namespace: BTreeMap<&'a [u8], [Option<usize>; 2]> = BTreeMap::new();
        for side in 0..2 {
            for (index, e) in view.roots[side].entries.iter().enumerate() {
                namespace.entry(e.relative).or_insert([None; 2])[side] = Some(index);
            }
        }
        if namespace.len() > MAX_LINES {
            return Err(bad());
        }
        let mut lines: Vec<Line> = Vec::with_capacity(namespace.len());
        let mut covers: BTreeMap<&[u8], usize> = BTreeMap::new();
        for (path, locations) in namespace {
            let [a, b] = [0, 1].map(|s| locations[s].map(|i| &view.roots[s].entries[i]));
            let shared_directory = a.is_some_and(|e| e.identity.kind == Kind::Directory)
                && b.is_some_and(|e| e.identity.kind == Kind::Directory);
            let mut ancestor = path;
            let mut covered = None;
            while let Some(n) = ancestor.iter().rposition(|&v| v == b'/') {
                ancestor = &ancestor[..n];
                if let Some(&i) = covers.get(ancestor) {
                    covered = Some(i);
                    break;
                }
            }
            let proposal = if let Some(i) = covered {
                Proposal::CoveredBy(i)
            } else if path.is_empty() && !options.root_metadata {
                Proposal::MergeDirectory
            } else {
                classify(a, b, view.mode, options.metadata)
            };
            let directory = a.is_some_and(|e| e.identity.kind == Kind::Directory)
                || b.is_some_and(|e| e.identity.kind == Kind::Directory);
            if !path.is_empty()
                && directory
                && matches!(
                    proposal,
                    Proposal::CopyFrom(_)
                        | Proposal::Retain
                        | Proposal::Review(Conflict::Type | Conflict::DestinationOnly)
                )
            {
                covers.insert(path, lines.len());
            }
            let decision = match proposal {
                Proposal::CopyFrom(_) => Decision::Include,
                Proposal::Review(_) => Decision::Pending,
                _ => Decision::None,
            };
            lines.push(Line {
                locations,
                proposal,
                decision,
                shared_directory,
            });
        }
        Ok(Self {
            record,
            view,
            options,
            lines,
            instance,
            revision: 0,
        })
    }
    pub fn record(&self) -> &'a Record {
        self.record
    }
    pub fn options(&self) -> Options {
        self.options
    }
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }
    pub fn name(&self, row: usize) -> Option<&'a [u8]> {
        let line = self.lines.get(row)?;
        for side in 0..2 {
            if let Some(i) = line.locations[side] {
                return Some(self.view.roots[side].entries[i].relative);
            }
        }
        None
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn receipt(&self, row: usize) -> io::Result<Receipt> {
        if row >= self.lines.len() {
            return Err(bad());
        }
        Ok(Receipt {
            instance: self.instance,
            revision: self.revision,
            row,
        })
    }
    pub fn choose(&mut self, receipt: Receipt, decision: Decision) -> io::Result<()> {
        if receipt.instance != self.instance || receipt.revision != self.revision {
            return Err(bad());
        }
        let line = self.lines.get_mut(receipt.row).ok_or_else(bad)?;
        if !allowed(line, decision, self.view.mode) {
            return Err(bad());
        }
        if line.decision != decision {
            let next = self.revision.checked_add(1).ok_or_else(bad)?;
            line.decision = decision;
            self.revision = next;
        }
        Ok(())
    }
    pub fn unresolved(&self) -> usize {
        self.lines
            .iter()
            .filter(|l| l.decision == Decision::Pending)
            .count()
    }
    /// Logical effects only; MergeDirectory/CoveredBy do not mean a syscall.
    /// Hardlink policy, metadata scope and recoverable publication are required
    /// executor contracts, not capabilities proven by a resolved plan.
    pub fn effect(&self, row: usize) -> io::Result<Effect> {
        let l = self.lines.get(row).ok_or_else(bad)?;
        Ok(match (l.proposal, l.decision) {
            (_, Decision::Pending) => return Err(bad()),
            (Proposal::MergeDirectory, _) => Effect::MergeDirectory,
            (Proposal::CoveredBy(i), _) => Effect::CoveredBy(i),
            (Proposal::CopyFrom(s), Decision::Include) => Effect::CopyFrom(s),
            (Proposal::Review(Conflict::DestinationOnly), Decision::Retire) => {
                Effect::PreserveAndRetire
            }
            (Proposal::Review(Conflict::Metadata), Decision::Take(s)) => {
                Effect::PreserveAndSetMetadataFrom(s)
            }
            (Proposal::Review(_), Decision::Take(s)) => Effect::PreserveAndReplaceFrom(s),
            (_, Decision::Skip) if l.shared_directory => Effect::MergeDirectory,
            _ => Effect::None,
        })
    }
    /// Content effects first; directory metadata after children, deepest first.
    /// This is not a complete physical schedule for hardlink/topology effects.
    pub fn effect_order(&self) -> io::Result<Vec<usize>> {
        let mut out = Vec::new();
        for i in 0..self.lines.len() {
            if matches!(
                self.effect(i)?,
                Effect::CopyFrom(_)
                    | Effect::PreserveAndReplaceFrom(_)
                    | Effect::PreserveAndSetMetadataFrom(_)
                    | Effect::PreserveAndRetire
            ) {
                out.push(i);
            }
        }
        out.sort_by_key(|&i| {
            let last = self.lines[i].shared_directory
                && matches!(self.effect(i), Ok(Effect::PreserveAndSetMetadataFrom(_)));
            (
                last,
                if last {
                    usize::MAX - {
                        let name = self.name(i).unwrap_or_default();
                        usize::from(!name.is_empty()) + name.iter().filter(|&&v| v == b'/').count()
                    }
                } else {
                    i
                },
            )
        });
        Ok(out)
    }
    pub fn encode(&self) -> io::Result<Vec<u8>> {
        let mut out = Vec::with_capacity(8 + 32 + 3 + 4 + self.lines.len() + 32);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&self.record.seal());
        out.push(match self.options.metadata {
            Metadata::Ignore => 0,
            Metadata::Basic => 1,
            Metadata::ExactObserved => 2,
        });
        out.push(u8::from(self.options.root_metadata));
        out.push(match self.options.hardlinks {
            Hardlinks::IndependentNames => 0,
            Hardlinks::PreserveInternal => 1,
        });
        out.extend_from_slice(&(self.lines.len() as u32).to_le_bytes());
        for l in &self.lines {
            if !allowed(l, l.decision, self.view.mode) {
                return Err(bad());
            }
            out.push(decision_byte(l.decision));
        }
        let digest = seal(&out);
        out.extend_from_slice(&digest);
        Ok(out)
    }
    pub fn seal(&self) -> io::Result<[u8; 32]> {
        let bytes = self.encode()?;
        bytes[bytes.len() - 32..].try_into().map_err(|_| bad())
    }
    pub fn decode(record: &'a Record, bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < 80 || bytes.len() > MAX_WIRE {
            return Err(bad());
        }
        let end = bytes.len() - 32;
        if seal(&bytes[..end]) != bytes[end..]
            || &bytes[..8] != MAGIC
            || bytes[8..40] != record.seal()
        {
            return Err(bad());
        }
        let metadata = match bytes[40] {
            0 => Metadata::Ignore,
            1 => Metadata::Basic,
            2 => Metadata::ExactObserved,
            _ => return Err(bad()),
        };
        let root_metadata = match bytes[41] {
            0 => false,
            1 => true,
            _ => return Err(bad()),
        };
        let hardlinks = match bytes[42] {
            0 => Hardlinks::IndependentNames,
            1 => Hardlinks::PreserveInternal,
            _ => return Err(bad()),
        };
        let count = u32::from_le_bytes(bytes[43..47].try_into().map_err(|_| bad())?) as usize;
        if count > MAX_LINES || 47 + count != end {
            return Err(bad());
        }
        let mut plan = Self::new(
            record,
            Options {
                metadata,
                root_metadata,
                hardlinks,
            },
        )?;
        if count != plan.lines.len() {
            return Err(bad());
        }
        for (l, &byte) in plan.lines.iter_mut().zip(&bytes[47..end]) {
            let d = decision(byte)?;
            if !allowed(l, d, plan.view.mode) {
                return Err(bad());
            }
            l.decision = d;
        }
        Ok(plan)
    }
}

/// A resolved logical plan freshly tied to live evidence. This borrows the plan
/// so choices cannot change while the binding is held. No approval/barrier.
#[cfg(target_os = "linux")]
pub struct BoundPlan<'p, 'r> {
    plan: &'p Plan<'r>,
    binding: crate::directory_sync_manifest::BoundRecord<'p>,
}
#[cfg(target_os = "linux")]
impl<'r> Plan<'r> {
    pub fn bind<'p>(
        &'p self,
        prepared: &'p crate::directory_sync_manifest::Prepared,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> io::Result<BoundPlan<'p, 'r>> {
        if self.unresolved() != 0 {
            return Err(bad());
        }
        let binding = prepared.bind_record(self.record, cancel)?;
        Ok(BoundPlan {
            plan: self,
            binding,
        })
    }
}
/// Comparison data for a particular job, evidence and complete choices.
/// Matching this key is not human approval or proof of durable storage.
pub struct ReviewKey {
    job: [u8; 16],
    evidence: [u8; 32],
    plan: [u8; 32],
}
pub struct StoredReviewKey {
    job: [u8; 16],
    evidence: [u8; 32],
    plan: [u8; 32],
}
const REVIEW_MAGIC: &[u8; 8] = b"FLSREV\0\x01";
#[cfg(target_os = "linux")]
impl<'p, 'r> BoundPlan<'p, 'r> {
    pub fn plan(&self) -> &Plan<'r> {
        self.plan
    }
    pub fn record_binding(&self) -> &crate::directory_sync_manifest::BoundRecord<'p> {
        &self.binding
    }
    pub fn review_key(&self, job: [u8; 16]) -> io::Result<ReviewKey> {
        if job == [0; 16] {
            return Err(bad());
        }
        Ok(ReviewKey {
            job,
            evidence: self.plan.record.seal(),
            plan: self.plan.seal()?,
        })
    }
}
impl ReviewKey {
    pub fn encode(&self) -> [u8; 88] {
        let mut out = [0; 88];
        out[..8].copy_from_slice(REVIEW_MAGIC);
        out[8..24].copy_from_slice(&self.job);
        out[24..56].copy_from_slice(&self.evidence);
        out[56..].copy_from_slice(&self.plan);
        out
    }
}
impl StoredReviewKey {
    pub fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() != 88 || &bytes[..8] != REVIEW_MAGIC || bytes[8..24] == [0; 16] {
            return Err(bad());
        }
        Ok(Self {
            job: bytes[8..24].try_into().map_err(|_| bad())?,
            evidence: bytes[24..56].try_into().map_err(|_| bad())?,
            plan: bytes[56..88].try_into().map_err(|_| bad())?,
        })
    }
    pub fn matches(&self, key: &ReviewKey) -> bool {
        self.job == key.job && self.evidence == key.evidence && self.plan == key.plan
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::{
        directory_sync_manifest::{self, Prepared},
        directory_sync_wire::Intent,
    };
    use std::{
        fs,
        os::unix::{
            ffi::OsStringExt,
            fs::{PermissionsExt, symlink},
        },
        path::PathBuf,
        sync::atomic::AtomicBool,
    };
    struct Fixture {
        base: PathBuf,
        paths: [PathBuf; 2],
    }
    impl Fixture {
        fn new() -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let base = std::env::temp_dir().join(format!(
                "files-sync-plan-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            let paths = [base.join("a"), base.join("b")];
            for p in &paths {
                fs::create_dir_all(p).unwrap();
            }
            Self { base, paths }
        }
        fn prepare(&self, mode: Mode) -> Prepared {
            directory_sync_manifest::capture(
                Intent {
                    version: 1,
                    id: 7,
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
        fn file(&self, side: usize, name: &str, data: &[u8]) {
            fs::write(self.paths[side].join(name), data).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }
    fn ignored() -> Options {
        Options {
            metadata: Metadata::Ignore,
            ..Options::default()
        }
    }
    fn row(p: &Plan<'_>, name: &[u8]) -> usize {
        (0..p.lines.len())
            .find(|&i| p.name(i) == Some(name))
            .unwrap()
    }
    fn choose(p: &mut Plan<'_>, name: &[u8], d: Decision) {
        let i = row(p, name);
        p.choose(p.receipt(i).unwrap(), d).unwrap();
    }
    #[test]
    fn all_five_modes_and_direction_constraints() {
        for mode in [
            Mode::Update(Side::Left),
            Mode::Update(Side::Right),
            Mode::Mirror(Side::Left),
            Mode::Mirror(Side::Right),
            Mode::Merge,
        ] {
            let f = Fixture::new();
            f.file(0, "left", b"a");
            f.file(1, "right", b"b");
            f.file(0, "both", b"a");
            f.file(1, "both", b"b");
            let prepared = f.prepare(mode);
            let record = prepared.record().unwrap();
            let mut p = Plan::new(&record, ignored()).unwrap();
            for (name, side) in [
                (b"left".as_slice(), Side::Left),
                (b"right".as_slice(), Side::Right),
            ] {
                let i = row(&p, name);
                let expected = match mode {
                    Mode::Merge => Proposal::CopyFrom(side),
                    Mode::Update(s) | Mode::Mirror(s) if s == side => Proposal::CopyFrom(side),
                    Mode::Update(_) => Proposal::Retain,
                    Mode::Mirror(_) => Proposal::Review(Conflict::DestinationOnly),
                };
                assert_eq!(p.lines[i].proposal, expected);
                if expected == Proposal::Review(Conflict::DestinationOnly) {
                    assert!(
                        p.choose(p.receipt(i).unwrap(), Decision::Take(side))
                            .is_err()
                    );
                    p.choose(p.receipt(i).unwrap(), Decision::Retire).unwrap();
                    assert_eq!(p.effect(i).unwrap(), Effect::PreserveAndRetire);
                }
            }
            let i = row(&p, b"both");
            assert!(p.effect_order().is_err());
            for side in [Side::Left, Side::Right] {
                let result = p.choose(p.receipt(i).unwrap(), Decision::Take(side));
                assert_eq!(result.is_ok(), side_allowed(mode, side));
            }
            assert_eq!(p.unresolved(), 0);
            assert!(p.effect_order().is_ok());
        }
    }
    #[test]
    fn directory_coverage_survives_raw_sort_interleaving_and_skips() {
        let f = Fixture::new();
        fs::create_dir(f.paths[0].join("a")).unwrap();
        f.file(0, "a/child", b"x");
        f.file(0, "a-", b"x");
        fs::create_dir(f.paths[1].join("z")).unwrap();
        f.file(1, "z/child", b"x");
        let prepared = f.prepare(Mode::Mirror(Side::Left));
        let record = prepared.record().unwrap();
        let mut p = Plan::new(&record, ignored()).unwrap();
        let a = row(&p, b"a");
        assert_eq!(
            p.lines[row(&p, b"a/child")].proposal,
            Proposal::CoveredBy(a)
        );
        assert_eq!(
            p.lines[row(&p, b"a-")].proposal,
            Proposal::CopyFrom(Side::Left)
        );
        choose(&mut p, b"a", Decision::Skip);
        assert_eq!(p.effect(a).unwrap(), Effect::None);
        assert!(
            p.choose(p.receipt(row(&p, b"a/child")).unwrap(), Decision::Include)
                .is_err()
        );
        let z = row(&p, b"z");
        assert_eq!(
            p.lines[row(&p, b"z/child")].proposal,
            Proposal::CoveredBy(z)
        );
        choose(&mut p, b"z", Decision::Retire);
        assert_eq!(p.effect_order().unwrap(), vec![row(&p, b"a-"), z]);
    }
    #[test]
    fn type_conflicts_cover_subtree_and_links_remain_links() {
        let f = Fixture::new();
        fs::create_dir(f.paths[0].join("type")).unwrap();
        f.file(0, "type/child", b"x");
        f.file(1, "type", b"y");
        symlink("missing-a", f.paths[0].join("link")).unwrap();
        symlink("missing-b", f.paths[1].join("link")).unwrap();
        let prepared = f.prepare(Mode::Merge);
        let record = prepared.record().unwrap();
        let mut p = Plan::new(&record, ignored()).unwrap();
        assert_eq!(
            p.lines[row(&p, b"type")].proposal,
            Proposal::Review(Conflict::Type)
        );
        assert_eq!(
            p.lines[row(&p, b"link")].proposal,
            Proposal::Review(Conflict::Link)
        );
        choose(&mut p, b"type", Decision::Take(Side::Right));
        assert_eq!(
            p.effect(row(&p, b"type")).unwrap(),
            Effect::PreserveAndReplaceFrom(Side::Right)
        );
        assert_eq!(
            p.effect(row(&p, b"type/child")).unwrap(),
            Effect::CoveredBy(row(&p, b"type"))
        );
    }
    #[test]
    fn metadata_policy_shared_directories_and_root_order() {
        let f = Fixture::new();
        for side in 0..2 {
            fs::create_dir_all(f.paths[side].join("d/sub")).unwrap();
            f.file(side, "d/sub/file", b"same");
        }
        for name in ["", "d", "d/sub"] {
            fs::set_permissions(f.paths[0].join(name), fs::Permissions::from_mode(0o700)).unwrap();
            fs::set_permissions(f.paths[1].join(name), fs::Permissions::from_mode(0o755)).unwrap();
        }
        let prepared = f.prepare(Mode::Merge);
        let record = prepared.record().unwrap();
        let ignored = Plan::new(&record, ignored()).unwrap();
        assert_eq!(ignored.unresolved(), 0);
        let mut p = Plan::new(
            &record,
            Options {
                root_metadata: true,
                ..Options::default()
            },
        )
        .unwrap();
        for name in [b"".as_slice(), b"d", b"d/sub", b"d/sub/file"] {
            let i = row(&p, name);
            if p.lines[i].decision == Decision::Pending {
                choose(&mut p, name, Decision::Take(Side::Left));
            }
        }
        let order = p.effect_order().unwrap();
        assert_eq!(
            &order[order.len() - 3..],
            &[row(&p, b"d/sub"), row(&p, b"d"), row(&p, b"")]
        );
        choose(&mut p, b"d", Decision::Skip);
        assert_eq!(p.effect(row(&p, b"d")).unwrap(), Effect::MergeDirectory);
        assert!(p.effect_order().unwrap().contains(&row(&p, b"d/sub")));
        let exact = Plan::new(
            &record,
            Options {
                metadata: Metadata::ExactObserved,
                ..Options::default()
            },
        )
        .unwrap();
        assert!(exact.unresolved() >= 2);
    }
    #[test]
    fn directory_mtime_is_ignored_by_basic_but_exact_observes_it() {
        let f = Fixture::new();
        for side in 0..2 {
            fs::create_dir(f.paths[side].join("d")).unwrap();
            let file = fs::File::open(f.paths[side].join("d")).unwrap();
            file.set_times(fs::FileTimes::new().set_modified(
                std::time::UNIX_EPOCH + std::time::Duration::from_secs(100 + side as u64),
            ))
            .unwrap();
        }
        let prepared = f.prepare(Mode::Merge);
        let record = prepared.record().unwrap();
        let basic = Plan::new(&record, Options::default()).unwrap();
        assert_eq!(
            basic.lines[row(&basic, b"d")].proposal,
            Proposal::MergeDirectory
        );
        let exact = Plan::new(
            &record,
            Options {
                metadata: Metadata::ExactObserved,
                ..Options::default()
            },
        )
        .unwrap();
        assert_eq!(
            exact.lines[row(&exact, b"d")].proposal,
            Proposal::Review(Conflict::Metadata)
        );
    }
    #[test]
    fn receipts_reject_old_instance_revision_and_reverted_choices() {
        let f = Fixture::new();
        f.file(0, "x", b"x");
        let prepared = f.prepare(Mode::Merge);
        let record = prepared.record().unwrap();
        let mut p = Plan::new(&record, ignored()).unwrap();
        let original = p.encode().unwrap();
        let i = row(&p, b"x");
        let old = p.receipt(i).unwrap();
        p.choose(old, Decision::Skip).unwrap();
        assert!(p.choose(old, Decision::Include).is_err());
        p.choose(p.receipt(i).unwrap(), Decision::Include).unwrap();
        assert_eq!(original, p.encode().unwrap());
        assert!(p.choose(old, Decision::Skip).is_err());
        let mut decoded = Plan::decode(&record, &original).unwrap();
        assert!(
            decoded
                .choose(p.receipt(i).unwrap(), Decision::Skip)
                .is_err()
        );
        assert!(p.receipt(usize::MAX).is_err());
    }
    #[test]
    fn canonical_roundtrip_raw_names_and_policy_seals() {
        let f = Fixture::new();
        let name = std::ffi::OsString::from_vec(vec![b'.', 255, b'\n']);
        fs::write(f.paths[0].join(&name), b"x").unwrap();
        let prepared = f.prepare(Mode::Merge);
        let record = prepared.record().unwrap();
        let p = Plan::new(&record, ignored()).unwrap();
        assert_eq!(p.name(1).unwrap(), name.as_encoded_bytes());
        let bytes = p.encode().unwrap();
        assert_eq!(bytes.capacity(), bytes.len());
        assert_eq!(
            bytes,
            Plan::decode(&record, &bytes).unwrap().encode().unwrap()
        );
        let changed = Plan::new(
            &record,
            Options {
                hardlinks: Hardlinks::IndependentNames,
                ..ignored()
            },
        )
        .unwrap();
        assert_ne!(changed.seal().unwrap(), p.seal().unwrap());
        for n in 0..bytes.len() {
            assert!(Plan::decode(&record, &bytes[..n]).is_err());
        }
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(Plan::decode(&record, &extra).is_err());
    }
    fn reseal(bytes: &mut [u8]) {
        let end = bytes.len() - 32;
        let digest = seal(&bytes[..end]);
        bytes[end..].copy_from_slice(&digest);
    }
    #[test]
    fn recomputed_digest_cannot_validate_illegal_choices_or_shapes() {
        let f = Fixture::new();
        f.file(0, "x", b"x");
        let prepared = f.prepare(Mode::Merge);
        let record = prepared.record().unwrap();
        let p = Plan::new(&record, ignored()).unwrap();
        let bytes = p.encode().unwrap();
        for (offset, value) in [
            (0, 0),
            (8, 0),
            (40, 3),
            (41, 2),
            (42, 2),
            (43, 0),
            (47, 2),
            (48, 6),
            (48, 255),
        ] {
            let mut changed = bytes.clone();
            changed[offset] = if offset == 8 {
                changed[offset] ^ 1
            } else {
                value
            };
            reseal(&mut changed);
            assert!(Plan::decode(&record, &changed).is_err(), "offset {offset}");
        }
        let mut corrupted = bytes.clone();
        corrupted[48] = 3;
        assert!(Plan::decode(&record, &corrupted).is_err());
        let mut legal = bytes;
        legal[48] = 3;
        reseal(&mut legal);
        let decoded = Plan::decode(&record, &legal).unwrap();
        assert_eq!(decoded.lines[1].decision, Decision::Skip);
    }
    #[test]
    fn live_binding_rejects_pending_cancel_changed_files_and_foreign_evidence() {
        let f = Fixture::new();
        f.file(0, "x", b"a");
        f.file(1, "x", b"b");
        let prepared = f.prepare(Mode::Merge);
        let record = prepared.record().unwrap();
        let mut p = Plan::new(&record, ignored()).unwrap();
        assert!(p.bind(&prepared, &AtomicBool::new(false)).is_err());
        let history = Plan::decode(&record, &p.encode().unwrap()).unwrap();
        assert_eq!(history.unresolved(), 1);
        assert!(history.bind(&prepared, &AtomicBool::new(false)).is_err());
        choose(&mut p, b"x", Decision::Take(Side::Left));
        assert!(p.bind(&prepared, &AtomicBool::new(true)).is_err());
        let key = p
            .bind(&prepared, &AtomicBool::new(false))
            .unwrap()
            .review_key([7; 16])
            .unwrap();
        let stored = StoredReviewKey::decode(&key.encode()).unwrap();
        assert!(stored.matches(&key));
        assert!(
            p.bind(&prepared, &AtomicBool::new(false))
                .unwrap()
                .review_key([0; 16])
                .is_err()
        );
        let other = p
            .bind(&prepared, &AtomicBool::new(false))
            .unwrap()
            .review_key([8; 16])
            .unwrap();
        assert!(!stored.matches(&other));
        choose(&mut p, b"x", Decision::Skip);
        let skipped = p
            .bind(&prepared, &AtomicBool::new(false))
            .unwrap()
            .review_key([7; 16])
            .unwrap();
        assert!(!stored.matches(&skipped));
        f.file(0, "x", b"changed");
        assert!(stored.matches(&key));
        assert!(p.bind(&prepared, &AtomicBool::new(false)).is_err());
        let foreign = f.prepare(Mode::Merge);
        assert!(p.bind(&foreign, &AtomicBool::new(false)).is_err());
    }
    #[test]
    fn review_key_rejects_old_format_and_zero_job() {
        assert!(StoredReviewKey::decode(&[0; 88]).is_err());
        let mut bytes = [0; 88];
        bytes[..8].copy_from_slice(REVIEW_MAGIC);
        assert!(StoredReviewKey::decode(&bytes).is_err());
        bytes[8] = 1;
        assert!(StoredReviewKey::decode(&bytes).is_ok());
        assert!(StoredReviewKey::decode(&bytes[..87]).is_err());
        bytes[0] = 0;
        assert!(StoredReviewKey::decode(&bytes).is_err());
    }
    #[test]
    fn maximum_disjoint_names_have_no_truncation() {
        let f = Fixture::new();
        for side in 0..2 {
            for n in 0..crate::directory_compare::MAX_ITEMS {
                f.file(side, &format!("{side}-{n:04}-{}", "x".repeat(118)), b"");
            }
        }
        let prepared = f.prepare(Mode::Merge);
        let record = prepared.record().unwrap();
        assert!(record.bytes().len() > 4 * 1024 * 1024);
        let p = Plan::new(&record, ignored()).unwrap();
        assert_eq!(p.lines.len(), MAX_LINES);
        assert_eq!(p.effect_order().unwrap().len(), MAX_LINES - 1);
        let bytes = p.encode().unwrap();
        assert_eq!(bytes.len(), MAX_WIRE);
        assert_eq!(
            Plan::decode(&record, &bytes).unwrap().lines.len(),
            MAX_LINES
        );
    }
}
