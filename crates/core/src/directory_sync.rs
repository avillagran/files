//! Fresh, bounded synchronization REVIEW. No executor or mutation authority.
//! Replacement/removal choices require preserving the destination first; the
//! existing no-clobber copy engine alone cannot implement those choices.
use crate::{
    directory_compare::{self as compare, Difference, Kind, Limits, Progress, Report, Row},
    relative_source::DirectoryIdentity,
};
use std::{
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Left,
    Right,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Add source-only names; review differences; retain destination-only names.
    Update(Side),
    /// As Update, but destination-only names require explicit removal review.
    Mirror(Side),
    /// Add unique names both ways. Without a baseline, neither side wins conflicts
    /// and absence cannot be interpreted as an intentional deletion.
    Merge,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conflict {
    Content,
    Link,
    Type,
    Metadata,
    DestinationOnly,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proposal {
    Unchanged,
    Retain,
    CopyFrom(Side),
    Review(Conflict),
    /// No independent operation: an ancestor is copied/removed/replaced or
    /// requires a type decision. The index always names an earlier review row.
    CoveredBy(usize),
    /// Any partial/changed/unsupported inventory blocks the complete review.
    Blocked,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Pending,
    Skip,
    /// Requires a recovery artifact for the old destination before publication.
    PreserveAndReplaceFrom(Side),
    /// Basic mode/mtime only, with recoverable old metadata; not ACL/xattr/owner.
    PreserveAndSetMetadataFrom(Side),
    /// Mirror destination only, and never an immediate irreversible unlink.
    PreserveAndRemove,
}
#[derive(Debug)]
pub struct Entry {
    observation: Row,
    proposal: Proposal,
    choice: Choice,
}
impl Entry {
    pub fn observation(&self) -> &Row {
        &self.observation
    }
    pub fn proposal(&self) -> Proposal {
        self.proposal
    }
    pub fn choice(&self) -> Choice {
        self.choice
    }
}
/// Private construction prevents an old, caller-supplied comparison report from
/// being relabeled as a fresh review. Even resolved reviews are NOT apply tokens:
/// execution must pin and validate fresh per-object revisions and recovery state.
#[derive(Debug)]
pub struct Review {
    paths: [PathBuf; 2],
    roots: [DirectoryIdentity; 2],
    mode: Mode,
    progress: Progress,
    entries: Vec<Entry>,
}
impl Review {
    /// Discard choices and consume into read-only observation history. There is
    /// deliberately no public inverse accepting a caller-supplied Report.
    pub fn into_observation(self) -> Report {
        Report {
            roots: self.roots,
            progress: self.progress,
            rows: self.entries.into_iter().map(|e| e.observation).collect(),
        }
    }
    pub fn paths(&self) -> [&Path; 2] {
        [&self.paths[0], &self.paths[1]]
    }
    pub fn roots(&self) -> [DirectoryIdentity; 2] {
        self.roots
    }
    pub fn mode(&self) -> Mode {
        self.mode
    }
    pub fn progress(&self) -> Progress {
        self.progress
    }
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    /// Counts only decisions, never a promise that execution can start.
    pub fn unresolved(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| matches!(e.proposal, Proposal::Review(_)) && e.choice == Choice::Pending)
            .count()
    }
    pub fn blocked(&self) -> bool {
        !self.progress.complete()
    }
    pub fn choose(&mut self, index: usize, choice: Choice) -> io::Result<()> {
        let entry = self
            .entries
            .get_mut(index)
            .ok_or_else(|| invalid("Fila inválida"))?;
        let Proposal::Review(conflict) = entry.proposal else {
            return Err(invalid("La fila no admite una decisión independiente"));
        };
        let allowed_side = |side| match self.mode {
            Mode::Update(source) | Mode::Mirror(source) => source == side,
            Mode::Merge => true,
        };
        let allowed = match choice {
            Choice::Pending | Choice::Skip => true,
            Choice::PreserveAndReplaceFrom(side) => {
                matches!(
                    conflict,
                    Conflict::Content | Conflict::Link | Conflict::Type
                ) && allowed_side(side)
            }
            Choice::PreserveAndSetMetadataFrom(side) => {
                conflict == Conflict::Metadata && allowed_side(side)
            }
            Choice::PreserveAndRemove => {
                conflict == Conflict::DestinationOnly && matches!(self.mode, Mode::Mirror(_))
            }
        };
        if !allowed {
            return Err(invalid(
                "Decisión incompatible con el conflicto y la dirección",
            ));
        }
        entry.choice = choice;
        Ok(())
    }
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn check(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Preparación cancelada",
        ))
    } else {
        Ok(())
    }
}
fn independent_paths(paths: [&Path; 2]) -> io::Result<[PathBuf; 2]> {
    for path in paths {
        crate::navigation_history::validate(path).map_err(invalid)?;
        if path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(invalid("La raíz contiene un componente '..'"));
        }
        // Final symlink roots are not supported by the comparison backend.
        if !std::fs::symlink_metadata(path)?.is_dir() {
            return Err(invalid("La raíz debe ser un directorio sin enlace final"));
        }
    }
    let canonical = [
        std::fs::canonicalize(paths[0])?,
        std::fs::canonicalize(paths[1])?,
    ];
    if canonical[0].starts_with(&canonical[1]) || canonical[1].starts_with(&canonical[0]) {
        return Err(invalid("Las raíces coinciden o se contienen"));
    }
    Ok(canonical)
}
/// Runs new inventory/hash observations, never consumes the previous UI report.
/// Worker/helper use only: filesystem calls may block, including cancellation.
/// Canonical containment and equal root identities are rejected; this is not a
/// proof against bind-mount/submount aliases, which apply must independently gate.
pub fn prepare(
    paths: [&Path; 2],
    expected: Option<[DirectoryIdentity; 2]>,
    mode: Mode,
    limits: Limits,
    cancel: &AtomicBool,
    progress: impl FnMut(Progress) -> io::Result<()>,
) -> io::Result<Review> {
    check(cancel)?;
    let canonical = independent_paths(paths)?;
    let report = compare::compare(paths[0], paths[1], expected, limits, cancel, progress)?;
    check(cancel)?;
    let [left, right] =
        paths.map(|p| DirectoryIdentity::from_metadata(&std::fs::symlink_metadata(p)?));
    let current = [left?, right?];
    if independent_paths(paths)? != canonical
        || report.roots[0] == report.roots[1]
        || current != report.roots
    {
        return Err(io::Error::other(
            "Las raíces cambiaron o son el mismo directorio",
        ));
    }
    classify(paths, mode, report, cancel)
}
fn proposal(row: &Row, mode: Mode) -> Proposal {
    match row.difference {
        Difference::Unknown => Proposal::Blocked,
        Difference::OnlyLeft | Difference::OnlyRight => {
            let present = if row.difference == Difference::OnlyLeft {
                Side::Left
            } else {
                Side::Right
            };
            match mode {
                Mode::Merge => Proposal::CopyFrom(present),
                Mode::Update(source) | Mode::Mirror(source) if source == present => {
                    Proposal::CopyFrom(present)
                }
                Mode::Update(_) => Proposal::Retain,
                Mode::Mirror(_) => Proposal::Review(Conflict::DestinationOnly),
            }
        }
        Difference::Type => Proposal::Review(Conflict::Type),
        Difference::Content => Proposal::Review(Conflict::Content),
        Difference::Link => Proposal::Review(Conflict::Link),
        Difference::Equal | Difference::Children => {
            if row.metadata_differs {
                Proposal::Review(Conflict::Metadata)
            } else {
                Proposal::Unchanged
            }
        }
    }
}
pub(crate) fn classify(
    paths: [&Path; 2],
    mode: Mode,
    report: Report,
    cancel: &AtomicBool,
) -> io::Result<Review> {
    let blocked = !report.progress.complete();
    let mut entries: Vec<Entry> = Vec::with_capacity(report.rows.len());
    // Sorted namespace guarantees ancestors precede descendants. Maintain only
    // open subtree scopes (bounded depth), not quadratic all-prior-row scans.
    let mut scopes: Vec<usize> = Vec::new();
    for row in report.rows {
        check(cancel)?;
        while scopes
            .last()
            .is_some_and(|&i| !row.path.starts_with(&entries[i].observation.path))
        {
            scopes.pop();
        }
        let p = if blocked {
            Proposal::Blocked
        } else if let Some(&i) = scopes.last() {
            Proposal::CoveredBy(i)
        } else {
            proposal(&row, mode)
        };
        let subtree = !blocked
            && scopes.is_empty()
            && (row.left == Some(Kind::Directory) || row.right == Some(Kind::Directory))
            && matches!(
                p,
                Proposal::CopyFrom(_)
                    | Proposal::Retain
                    | Proposal::Review(Conflict::Type | Conflict::DestinationOnly)
            );
        if subtree {
            scopes.push(entries.len());
        }
        entries.push(Entry {
            observation: row,
            proposal: p,
            choice: Choice::Pending,
        });
    }
    Ok(Review {
        paths: paths.map(Path::to_path_buf),
        roots: report.roots,
        mode,
        progress: report.progress,
        entries,
    })
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::{ffi::OsStringExt, fs::symlink},
        sync::atomic::AtomicU64,
    };
    struct Fixture {
        base: PathBuf,
        paths: [PathBuf; 2],
    }
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let base = std::env::temp_dir().join(format!(
                "files-sync-review-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let paths = [base.join("a"), base.join("b")];
            fs::create_dir_all(&paths[0]).unwrap();
            fs::create_dir(&paths[1]).unwrap();
            Self { base, paths }
        }
        fn refs(&self) -> [&Path; 2] {
            [&self.paths[0], &self.paths[1]]
        }
        fn write(&self, side: usize, name: &str, data: &[u8]) {
            let p = self.paths[side].join(name);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, data).unwrap();
            let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
            fs::File::options()
                .write(true)
                .open(p)
                .unwrap()
                .set_times(fs::FileTimes::new().set_modified(time))
                .unwrap();
        }
        fn prepare(&self, mode: Mode) -> Review {
            prepare(
                self.refs(),
                None,
                mode,
                Limits::default(),
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
    fn index(r: &Review, name: &Path) -> usize {
        r.entries
            .iter()
            .position(|e| e.observation.path == name)
            .unwrap()
    }
    fn at<'a>(r: &'a Review, name: &str) -> &'a Entry {
        &r.entries[index(r, Path::new(name))]
    }

    #[test]
    fn directions_and_modes_never_infer_bidirectional_deletion() {
        let f = Fixture::new();
        f.write(0, "a", b"A");
        f.write(1, "b", b"B");
        for source in [Side::Left, Side::Right] {
            let (src, dst) = if source == Side::Left {
                ("a", "b")
            } else {
                ("b", "a")
            };
            let update = f.prepare(Mode::Update(source));
            assert_eq!(at(&update, src).proposal(), Proposal::CopyFrom(source));
            assert_eq!(at(&update, dst).proposal(), Proposal::Retain);
            let mut mirror = f.prepare(Mode::Mirror(source));
            let i = index(&mirror, Path::new(dst));
            assert_eq!(mirror.unresolved(), 1);
            assert_eq!(
                mirror.entries[i].proposal(),
                Proposal::Review(Conflict::DestinationOnly)
            );
            assert!(
                mirror
                    .choose(i, Choice::PreserveAndReplaceFrom(source))
                    .is_err()
            );
            mirror.choose(i, Choice::PreserveAndRemove).unwrap();
            assert_eq!(mirror.unresolved(), 0);
        }
        let merge = f.prepare(Mode::Merge);
        assert_eq!(at(&merge, "a").proposal(), Proposal::CopyFrom(Side::Left));
        assert_eq!(at(&merge, "b").proposal(), Proposal::CopyFrom(Side::Right));
        assert_eq!(fs::read(f.paths[0].join("a")).unwrap(), b"A");
        assert_eq!(fs::read(f.paths[1].join("b")).unwrap(), b"B");
        assert!(!f.paths[0].join("b").exists());
        assert!(!f.paths[1].join("a").exists());
    }
    #[test]
    fn content_link_type_and_metadata_are_independent_explicit_conflicts() {
        let f = Fixture::new();
        for side in 0..2 {
            f.write(side, "same", b"same");
            f.write(side, "metadata", b"same");
        }
        f.write(0, "content", b"left");
        f.write(1, "content", b"right");
        f.write(0, "type/child", b"child");
        f.write(1, "type", b"file");
        symlink("left", f.paths[0].join("link")).unwrap();
        symlink("right", f.paths[1].join("link")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            f.paths[1].join("metadata"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        fs::set_permissions(
            f.paths[0].join("metadata"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        let mut r = f.prepare(Mode::Merge);
        assert!(!r.blocked());
        assert_eq!(r.unresolved(), 4);
        assert_eq!(at(&r, "same").proposal(), Proposal::Unchanged);
        for (name, conflict) in [
            ("content", Conflict::Content),
            ("link", Conflict::Link),
            ("type", Conflict::Type),
            ("metadata", Conflict::Metadata),
        ] {
            let i = index(&r, Path::new(name));
            assert_eq!(r.entries[i].proposal(), Proposal::Review(conflict));
            assert!(r.choose(i, Choice::PreserveAndRemove).is_err());
            for side in [Side::Left, Side::Right] {
                let choice = if conflict == Conflict::Metadata {
                    Choice::PreserveAndSetMetadataFrom(side)
                } else {
                    Choice::PreserveAndReplaceFrom(side)
                };
                r.choose(i, choice).unwrap();
            }
        }
        assert_eq!(
            at(&r, "type/child").proposal(),
            Proposal::CoveredBy(index(&r, Path::new("type")))
        );
        assert_eq!(r.unresolved(), 0);
        assert_eq!(fs::read(f.paths[0].join("content")).unwrap(), b"left");
        assert_eq!(fs::read(f.paths[1].join("type")).unwrap(), b"file");
        assert_eq!(
            fs::read_link(f.paths[0].join("link")).unwrap(),
            Path::new("left")
        );
    }
    #[test]
    fn subtree_proposals_do_not_duplicate_copy_or_removal_and_keep_raw_names() {
        let f = Fixture::new();
        f.write(0, "dir/deep/file", b"x");
        f.write(0, "dir-other/file", b"y");
        f.write(0, "dir/z", b"z");
        f.write(1, "extra/nested/file", b"extra");
        fs::create_dir(f.paths[0].join("empty")).unwrap();
        let raw = std::ffi::OsString::from_vec(vec![b'r', 255, b'\n']);
        fs::write(f.paths[0].join(&raw), b"raw").unwrap();
        let r = f.prepare(Mode::Mirror(Side::Left));
        assert_eq!(
            at(&r, "dir/deep/file").proposal(),
            Proposal::CoveredBy(index(&r, Path::new("dir")))
        );
        assert_eq!(
            at(&r, "dir/z").proposal(),
            Proposal::CoveredBy(index(&r, Path::new("dir")))
        );
        assert_eq!(
            at(&r, "dir-other/file").proposal(),
            Proposal::CoveredBy(index(&r, Path::new("dir-other")))
        );
        assert_eq!(
            at(&r, "extra/nested/file").proposal(),
            Proposal::CoveredBy(index(&r, Path::new("extra")))
        );
        assert_eq!(at(&r, "empty").proposal(), Proposal::CopyFrom(Side::Left));
        assert_eq!(
            r.entries[index(&r, Path::new(&raw))].proposal(),
            Proposal::CopyFrom(Side::Left)
        );
        assert_eq!(r.unresolved(), 1);
    }
    #[test]
    fn partial_inventory_or_read_budget_blocks_every_proposal() {
        let f = Fixture::new();
        f.write(0, "a", b"left");
        f.write(1, "a", b"rite");
        f.write(0, "extra", b"extra");
        for limits in [
            Limits {
                read: 0,
                ..Limits::default()
            },
            Limits {
                items: 1,
                ..Limits::default()
            },
        ] {
            let mut r = prepare(
                f.refs(),
                None,
                Mode::Mirror(Side::Left),
                limits,
                &AtomicBool::new(false),
                |_| Ok(()),
            )
            .unwrap();
            assert!(r.blocked());
            assert!(!r.entries.is_empty());
            assert!(r.entries.iter().all(|e| e.proposal() == Proposal::Blocked));
            assert!(r.choose(0, Choice::Skip).is_err());
        }
    }
    #[test]
    fn preparation_rereads_instead_of_trusting_old_comparison() {
        let f = Fixture::new();
        f.write(0, "file", b"same");
        f.write(1, "file", b"same");
        let old = compare::compare(
            &f.paths[0],
            &f.paths[1],
            None,
            Limits::default(),
            &AtomicBool::new(false),
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(old.rows[0].difference, Difference::Equal);
        f.write(1, "file", b"changed");
        assert_eq!(
            at(&f.prepare(Mode::Update(Side::Left)), "file").proposal(),
            Proposal::Review(Conflict::Content)
        );
    }
    #[test]
    fn changed_inventory_during_hashing_blocks_even_unrelated_missing_names() {
        let f = Fixture::new();
        for side in 0..2 {
            f.write(side, "same", b"same");
        }
        f.write(0, "unique", b"u");
        let mut changed = false;
        let r = prepare(
            f.refs(),
            None,
            Mode::Mirror(Side::Left),
            Limits::default(),
            &AtomicBool::new(false),
            |p| {
                if p.phase == 2 && !changed {
                    changed = true;
                    fs::write(f.paths[1].join("same"), b"change").unwrap();
                }
                Ok(())
            },
        )
        .unwrap();
        assert!(changed);
        assert!(r.blocked());
        assert_ne!(r.progress().reasons & compare::CHANGED, 0);
        assert_eq!(at(&r, "unique").proposal(), Proposal::Blocked);
    }
    #[test]
    fn replacing_a_root_while_comparing_never_yields_action_proposals() {
        let f = Fixture::new();
        for side in 0..2 {
            f.write(side, "same", b"same");
        }
        let mut changed = false;
        let result = prepare(
            f.refs(),
            None,
            Mode::Merge,
            Limits::default(),
            &AtomicBool::new(false),
            |p| {
                if p.phase == 2 && !changed {
                    changed = true;
                    fs::rename(&f.paths[1], f.base.join("old-b")).unwrap();
                    fs::create_dir(&f.paths[1]).unwrap();
                    fs::write(f.paths[1].join("replacement"), b"foreign").unwrap();
                }
                Ok(())
            },
        );
        assert!(changed);
        // Comparison can reject the root or return its explicitly changed view.
        if let Ok(r) = result {
            assert!(r.blocked());
            assert!(r.entries.iter().all(|e| e.proposal() == Proposal::Blocked));
        }
        assert_eq!(
            fs::read(f.paths[1].join("replacement")).unwrap(),
            b"foreign"
        );
        assert_eq!(fs::read(f.base.join("old-b/same")).unwrap(), b"same");
    }
    #[test]
    fn final_progress_sink_cannot_replace_a_root_and_publish_a_review() {
        let f = Fixture::new();
        let result = prepare(
            f.refs(),
            None,
            Mode::Merge,
            Limits::default(),
            &AtomicBool::new(false),
            |p| {
                if p.phase == 3 {
                    fs::rename(&f.paths[1], f.base.join("old-b")).unwrap();
                    fs::create_dir(&f.paths[1]).unwrap();
                }
                Ok(())
            },
        );
        assert!(result.is_err());
    }
    #[test]
    fn choices_are_directional_typed_reversible_and_never_bulk_resolve() {
        let f = Fixture::new();
        f.write(0, "a", b"left");
        f.write(1, "a", b"right");
        f.write(0, "b", b"left");
        f.write(1, "b", b"right");
        let mut r = f.prepare(Mode::Update(Side::Left));
        let i = index(&r, Path::new("a"));
        assert_eq!(r.unresolved(), 2);
        for bad in [
            Choice::PreserveAndReplaceFrom(Side::Right),
            Choice::PreserveAndRemove,
            Choice::PreserveAndSetMetadataFrom(Side::Left),
        ] {
            assert!(r.choose(i, bad).is_err());
            assert_eq!(r.entries[i].choice(), Choice::Pending);
        }
        r.choose(i, Choice::PreserveAndReplaceFrom(Side::Left))
            .unwrap();
        assert_eq!(r.unresolved(), 1);
        r.choose(i, Choice::Pending).unwrap();
        assert_eq!(r.unresolved(), 2);
        r.choose(i, Choice::Skip).unwrap();
        assert_eq!(r.unresolved(), 1);
        assert!(r.choose(usize::MAX, Choice::Skip).is_err());
    }
    #[test]
    fn equal_nested_and_shared_directory_metadata_do_not_hide_children() {
        let f = Fixture::new();
        for side in 0..2 {
            f.write(side, "shared/same", b"same");
        }
        f.write(0, "shared/unique", b"u");
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(f.paths[0].join("shared"), fs::Permissions::from_mode(0o755)).unwrap();
        fs::set_permissions(f.paths[1].join("shared"), fs::Permissions::from_mode(0o700)).unwrap();
        let r = f.prepare(Mode::Merge);
        assert_eq!(
            at(&r, "shared").proposal(),
            Proposal::Review(Conflict::Metadata)
        );
        assert_eq!(
            at(&r, "shared/unique").proposal(),
            Proposal::CopyFrom(Side::Left)
        );
        assert_eq!(at(&r, "shared/same").proposal(), Proposal::Unchanged);
    }
    #[test]
    fn cancellation_callback_errors_and_stale_root_identity_do_not_publish() {
        let f = Fixture::new();
        f.write(0, "x", b"x");
        let cancel = AtomicBool::new(true);
        assert_eq!(
            prepare(
                f.refs(),
                None,
                Mode::Merge,
                Limits::default(),
                &cancel,
                |_| Ok(())
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::Interrupted
        );
        cancel.store(false, Ordering::Relaxed);
        assert_eq!(
            prepare(
                f.refs(),
                None,
                Mode::Merge,
                Limits::default(),
                &cancel,
                |_| Err(io::Error::new(io::ErrorKind::BrokenPipe, "sink"))
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::BrokenPipe
        );
        let r = f.prepare(Mode::Merge);
        let mut ids = r.roots();
        ids[0].inode ^= 1;
        assert!(
            prepare(
                f.refs(),
                Some(ids),
                Mode::Merge,
                Limits::default(),
                &cancel,
                |_| Ok(())
            )
            .is_err()
        );
        assert_eq!(
            prepare(
                f.refs(),
                None,
                Mode::Merge,
                Limits::default(),
                &cancel,
                |_| {
                    cancel.store(true, Ordering::Relaxed);
                    Ok(())
                }
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::Interrupted
        );
    }
    #[test]
    fn overlapping_roots_parent_components_and_final_links_are_rejected() {
        let f = Fixture::new();
        fs::create_dir(f.paths[0].join("nested")).unwrap();
        symlink(&f.paths[0], f.base.join("alias")).unwrap();
        for paths in [
            [f.paths[0].clone(), f.paths[0].clone()],
            [f.paths[0].clone(), f.paths[0].join("nested")],
            [f.paths[0].join("nested"), f.paths[0].clone()],
            [f.paths[0].clone(), f.base.join("alias")],
            [f.paths[0].join("nested/.."), f.paths[1].clone()],
        ] {
            assert!(
                prepare(
                    [&paths[0], &paths[1]],
                    None,
                    Mode::Merge,
                    Limits::default(),
                    &AtomicBool::new(false),
                    |_| Ok(())
                )
                .is_err()
            );
        }
        // An ancestor alias is canonicalized before the overlap test.
        fs::create_dir(f.base.join("outer")).unwrap();
        symlink(&f.paths[0], f.base.join("outer/link")).unwrap();
        assert!(
            prepare(
                [&f.paths[0], &f.base.join("outer/link/nested")],
                None,
                Mode::Merge,
                Limits::default(),
                &AtomicBool::new(false),
                |_| Ok(())
            )
            .is_err()
        );
    }
}
