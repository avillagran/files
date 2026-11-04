//! Exact ctime transition reconciliation, not rename provenance or effect authority.
use super::*;
use std::collections::HashSet;
/// Combined inspected entries, including unrelated objects, across the batch.
pub const MAX_ALIAS_ENTRIES: usize = 4 * (MAX_ENTRIES + 1);
/// Current parent/name anchors. Caller validates their absolute path bindings.
pub struct AliasScope<'a> {
    pub inventory: &'a mut Inventory,
    pub parent: &'a File,
    pub name: &'a OsStr,
}
#[derive(Debug, PartialEq, Eq)]
pub struct AliasReport {
    pub occurrences: usize,
    pub payload_read: u64,
}
/// Exact transition from a successful full root rebase. Cannot be decoded or
/// constructed from public observations. The caller must identify its own rename;
/// even this private-field evidence does not prove who performed the syscall.
#[derive(Debug)]
pub struct RenameChange {
    pub(super) identity: Identity,
    pub(super) before: Revision,
    pub(super) after: Revision,
    pub(super) access: AccessTime,
    pub(super) content: [u8; 32],
    pub(super) metadata: [u8; 32],
    pub(super) mount: u64,
    pub(super) payload: u64,
}
impl RenameChange {
    pub fn payload_read(&self) -> u64 {
        self.payload
    }
    /// Rewalk ALL scopes before updating ANY recorded revisions. Each affected
    /// alias must match the exact old evidence and exact new revision, including
    /// ctime. Unrelated entries remain strict. No filesystem writes are performed.
    pub fn reconcile_aliases(
        &self,
        scopes: &mut [AliasScope<'_>],
        read: u64,
        cancel: &AtomicBool,
    ) -> io::Result<AliasReport> {
        self.reconcile_aliases_inner(scopes, read, cancel, None)
    }
    /// Same atomic recorded-state reconciliation with cooperative worker control.
    /// Pause/cancel checkpoints are shared with the existing inventory walker.
    pub fn reconcile_aliases_controlled(
        &self,
        scopes: &mut [AliasScope<'_>],
        read: u64,
        control: &Control,
    ) -> io::Result<AliasReport> {
        self.reconcile_aliases_inner(scopes, read, control.cancellation_flag(), Some(control))
    }
    fn reconcile_aliases_inner(
        &self,
        scopes: &mut [AliasScope<'_>],
        read: u64,
        cancel: &AtomicBool,
        control: Option<&Control>,
    ) -> io::Result<AliasReport> {
        check_control(cancel, control)?;
        if self.identity.kind == Kind::Directory
            || read > crate::directory_compare::MAX_READ
            || scopes.is_empty()
        {
            return Err(invalid("Grupo de alias inválido"));
        }
        let mut total = 0usize;
        let mut names = HashSet::new();
        for scope in scopes.iter() {
            check_control(cancel, control)?;
            component(scope.name.as_bytes())?;
            let inv = &scope.inventory;
            if inv.sync.is_none()
                || inv
                    .mounts
                    .as_ref()
                    .is_none_or(|m| m.len() != inv.entries.len())
            {
                return Err(invalid("Alias sin inventario sync completo"));
            }
            total = total
                .checked_add(inv.entries.len())
                .filter(|&n| n <= MAX_ALIAS_ENTRIES)
                .ok_or_else(|| invalid("Demasiadas entradas en grupo de alias"))?;
            let by_path: HashMap<&[u8], &Entry> = inv
                .entries
                .iter()
                .map(|e| (e.relative.as_slice(), e))
                .collect();
            let parent_identity = id(&scope.parent.metadata()?)?;
            if parent_identity.kind != Kind::Directory {
                return Err(invalid("Padre de alias no es directorio"));
            }
            let mut found = 0;
            for (i, e) in inv
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| e.identity == self.identity)
            {
                if e.revision != self.before
                    || e.access != self.access
                    || e.content_digest != self.content
                    || e.metadata_digest != self.metadata
                    || inv.mounts.as_ref().expect("checked")[i] != self.mount
                {
                    return Err(invalid(
                        "Captura de alias incompatible con transición exacta",
                    ));
                }
                let (parent, leaf) = if i == 0 {
                    (parent_identity, scope.name.as_bytes())
                } else {
                    let slash = e
                        .relative
                        .iter()
                        .rposition(|&b| b == b'/')
                        .ok_or_else(|| invalid("Alias sin padre"))?;
                    let p = by_path
                        .get(&e.relative[..slash])
                        .ok_or_else(|| invalid("Padre de alias ausente"))?;
                    (p.identity, &e.relative[slash + 1..])
                };
                if parent.kind != Kind::Directory
                    || !names.insert((parent.device, parent.inode, leaf.to_vec()))
                {
                    return Err(invalid("Nombre físico de alias duplicado"));
                }
                found += 1;
            }
            if found == 0 {
                return Err(invalid("Inventario sin alias de transición"));
            }
        }
        if names.len() as u64 > self.after.links {
            return Err(invalid("Alias exceden nlink observado"));
        }
        let mut payload = 0;
        for scope in scopes.iter() {
            payload += scope.inventory.verify_alias_transition(
                scope.parent,
                scope.name,
                self,
                read - payload,
                cancel,
                control,
            )?;
        }
        // Final current root/name checks across all scopes. This is not a
        // simultaneous filesystem snapshot; every future effect must reverify.
        for scope in scopes.iter() {
            check_control(cancel, control)?;
            let root = &scope.inventory.entries[0];
            let revision = if root.identity == self.identity {
                self.after
            } else {
                root.revision
            };
            stable(
                scope.parent,
                scope.name,
                &scope.inventory.root,
                root.identity,
                revision,
            )?;
            let mount = scope.inventory.mounts.as_ref().expect("checked")[0];
            if mount_id(&scope.inventory.root, None)? != mount
                || mount_id(scope.parent, Some(scope.name))? != mount
            {
                return Err(invalid("Montaje raíz de alias cambió"));
            }
        }
        check_control(cancel, control)?;
        // No failure/IO/allocation remains. Commit every in-memory owner together.
        for scope in scopes.iter_mut() {
            for e in &mut scope.inventory.entries {
                if e.identity == self.identity {
                    e.revision = self.after;
                }
            }
        }
        Ok(AliasReport {
            occurrences: names.len(),
            payload_read: payload,
        })
    }
}
impl Inventory {
    fn verify_alias_transition(
        &self,
        parent: &File,
        name: &OsStr,
        change: &RenameChange,
        read: u64,
        cancel: &AtomicBool,
        control: Option<&Control>,
    ) -> io::Result<u64> {
        let bounds = self.sync.ok_or_else(|| invalid("Alias sin captura sync"))?;
        let expected: HashMap<&[u8], &Entry> = self
            .entries
            .iter()
            .map(|e| (e.relative.as_slice(), e))
            .collect();
        let root = open(parent, name)?;
        if id(&root.metadata()?)? != self.entries[0].identity
            || id(&self.root.metadata()?)? != self.entries[0].identity
        {
            return Err(invalid("Raíz de alias sustituida"));
        }
        let mut count = 0;
        let mut walker = Walker::with_control(cancel, Some(&expected), control, |mut observed| {
            let old = *expected
                .get(observed.relative.as_slice())
                .ok_or_else(|| invalid("Namespace de alias cambió"))?;
            if old.identity == change.identity {
                if observed.revision != change.after {
                    return Err(invalid("Revisión actual no es el cambio propio exacto"));
                }
                observed.revision = change.before;
            }
            if &observed != old {
                return Err(invalid("Contenido/metadatos/objeto de alias cambió"));
            }
            count += 1;
            Ok(())
        });
        walker.set_sync(Some(SyncBounds { read, ..bounds }));
        walker.walk(parent, name, root, name.as_bytes(), 0)?;
        let payload = walker.payload_read;
        let topology = walker
            .topology
            .take()
            .ok_or_else(|| invalid("Alias sin topología"))?;
        drop(walker);
        if count != self.entries.len()
            || self
                .entries
                .iter()
                .zip(self.mounts.as_ref().expect("checked"))
                .any(|(e, m)| topology.get(&e.relative) != Some(m))
        {
            return Err(invalid("Alias incompleto o montaje distinto"));
        }
        Ok(payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{os::unix::fs::PermissionsExt, path::PathBuf};
    struct Fixture {
        base: PathBuf,
        parent: File,
        one: Inventory,
        two: Inventory,
        change: RenameChange,
    }
    impl Fixture {
        fn new() -> Self {
            let base = crate::persistence::test_dir("sync-alias");
            for d in ["one/sub", "two"] {
                fs::create_dir_all(base.join(d)).unwrap();
            }
            fs::write(base.join("source"), b"data").unwrap();
            fs::set_permissions(base.join("source"), fs::Permissions::from_mode(0o644)).unwrap();
            for n in ["one/sub/a", "one/b", "two/c"] {
                fs::hard_link(base.join("source"), base.join(n)).unwrap();
            }
            for n in ["one/unrelated", "two/unrelated"] {
                fs::write(base.join(n), b"other").unwrap();
            }
            let parent = File::open(&base).unwrap();
            let cancel = AtomicBool::new(false);
            let mut source = capture_for_sync(&parent, OsStr::new("source"), 4, &cancel).unwrap();
            let one = capture_for_sync(&parent, OsStr::new("one"), 9, &cancel).unwrap();
            let two = capture_for_sync(&parent, OsStr::new("two"), 9, &cancel).unwrap();
            fs::rename(base.join("source"), base.join("renamed")).unwrap();
            let change = source
                .reconcile_root_rename(&parent, OsStr::new("renamed"), 4, &cancel)
                .unwrap();
            Self {
                base,
                parent,
                one,
                two,
                change,
            }
        }
        fn reconcile(&mut self, read: u64, cancel: &AtomicBool) -> io::Result<AliasReport> {
            self.change.reconcile_aliases(
                &mut [
                    AliasScope {
                        inventory: &mut self.one,
                        parent: &self.parent,
                        name: OsStr::new("one"),
                    },
                    AliasScope {
                        inventory: &mut self.two,
                        parent: &self.parent,
                        name: OsStr::new("two"),
                    },
                ],
                read,
                cancel,
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }
    #[test]
    fn alias_batch_reconciles_every_known_name_and_keeps_other_entries_exact() {
        let mut f = Fixture::new();
        let cancel = AtomicBool::new(false);
        let original_read = f.one.payload_read();
        let untouched: Vec<_> = f
            .one
            .entries
            .iter()
            .filter(|e| e.identity != f.change.identity)
            .map(|e| format!("{e:?}"))
            .collect();
        assert!(f.one.verify(&f.parent, OsStr::new("one"), &cancel).is_err());
        assert_eq!(f.change.payload_read(), 4);
        assert_eq!(
            f.reconcile(18, &cancel).unwrap(),
            AliasReport {
                occurrences: 3,
                payload_read: 18
            }
        );
        f.one.verify(&f.parent, OsStr::new("one"), &cancel).unwrap();
        f.two.verify(&f.parent, OsStr::new("two"), &cancel).unwrap();
        assert_eq!(f.one.payload_read(), original_read);
        assert_eq!(
            f.one
                .entries
                .iter()
                .filter(|e| e.identity != f.change.identity)
                .map(|e| format!("{e:?}"))
                .collect::<Vec<_>>(),
            untouched
        );
        assert!(f.reconcile(18, &cancel).is_err()); // old transition cannot refresh a newer owner
    }
    #[test]
    fn alias_batch_failure_in_last_scope_never_partially_advances_first() {
        for case in [
            "content",
            "budget",
            "cancel",
            "ctime",
            "mount",
            "replacement",
        ] {
            let mut f = Fixture::new();
            let cancel = AtomicBool::new(case == "cancel");
            if case == "content" {
                fs::write(f.base.join("two/unrelated"), b"evil!").unwrap();
            }
            if case == "ctime" {
                fs::set_permissions(f.base.join("renamed"), fs::Permissions::from_mode(0o644))
                    .unwrap();
            }
            if case == "mount" {
                f.two.mounts.as_mut().unwrap()[0] ^= 1;
            }
            if case == "replacement" {
                fs::rename(f.base.join("two"), f.base.join("kept")).unwrap();
                fs::create_dir(f.base.join("two")).unwrap();
            }
            let one = format!("{:?}", f.one.entries);
            let two = format!("{:?}", f.two.entries);
            assert!(
                f.reconcile(if case == "budget" { 17 } else { 18 }, &cancel)
                    .is_err(),
                "{case}"
            );
            assert_eq!(format!("{:?}", f.one.entries), one, "{case}");
            assert_eq!(format!("{:?}", f.two.entries), two, "{case}");
        }
    }
    #[test]
    fn alias_batch_rejects_overlapping_physical_names_not_distinct_hardlink_names() {
        let mut f = Fixture::new();
        let cancel = AtomicBool::new(false);
        f.reconcile(18, &cancel).unwrap();
        let mut overlap = capture_for_sync(&f.parent, OsStr::new("one"), 9, &cancel).unwrap();
        let one_parent = File::open(f.base.join("one")).unwrap();
        let two_parent = File::open(f.base.join("two")).unwrap();
        let mut a = capture_for_sync(&one_parent, OsStr::new("b"), 4, &cancel).unwrap();
        let mut b = capture_for_sync(&two_parent, OsStr::new("c"), 4, &cancel).unwrap();
        let mut owner = capture_for_sync(&f.parent, OsStr::new("renamed"), 4, &cancel).unwrap();
        fs::rename(f.base.join("renamed"), f.base.join("again")).unwrap();
        f.change = owner
            .reconcile_root_rename(&f.parent, OsStr::new("again"), 4, &cancel)
            .unwrap();
        let original = format!("{:?}", f.one.entries);
        assert!(
            f.change
                .reconcile_aliases(
                    &mut [
                        AliasScope {
                            inventory: &mut f.one,
                            parent: &f.parent,
                            name: OsStr::new("one")
                        },
                        AliasScope {
                            inventory: &mut overlap,
                            parent: &f.parent,
                            name: OsStr::new("one")
                        }
                    ],
                    18,
                    &cancel
                )
                .is_err()
        );
        assert_eq!(format!("{:?}", f.one.entries), original);
        // Same inode, different captured physical names, real pre-rename evidence.
        assert_eq!(
            f.change
                .reconcile_aliases(
                    &mut [
                        AliasScope {
                            inventory: &mut a,
                            parent: &one_parent,
                            name: OsStr::new("b")
                        },
                        AliasScope {
                            inventory: &mut b,
                            parent: &two_parent,
                            name: OsStr::new("c")
                        }
                    ],
                    8,
                    &cancel
                )
                .unwrap()
                .occurrences,
            2
        );
    }
    #[test]
    fn alias_batch_requires_original_coherent_sync_capture_and_exact_new_revision() {
        let mut f = Fixture::new();
        let cancel = AtomicBool::new(false);
        let mut fresh = capture_for_sync(&f.parent, OsStr::new("one"), 9, &cancel).unwrap();
        assert!(
            f.change
                .reconcile_aliases(
                    &mut [AliasScope {
                        inventory: &mut fresh,
                        parent: &f.parent,
                        name: OsStr::new("one")
                    }],
                    9,
                    &cancel
                )
                .is_err()
        );
        let mut legacy = capture(&f.parent, OsStr::new("one"), &cancel).unwrap();
        assert!(
            f.change
                .reconcile_aliases(
                    &mut [AliasScope {
                        inventory: &mut legacy,
                        parent: &f.parent,
                        name: OsStr::new("one")
                    }],
                    9,
                    &cancel
                )
                .is_err()
        );
        f.one
            .entries
            .iter_mut()
            .find(|e| e.identity == f.change.identity)
            .unwrap()
            .content_digest[0] ^= 1;
        assert!(f.reconcile(18, &cancel).is_err());
        assert!(f.change.reconcile_aliases(&mut [], 0, &cancel).is_err());
    }
    #[test]
    fn alias_batch_preserves_distinct_raw_names_without_display_decoding() {
        use std::os::unix::ffi::OsStringExt;
        let base = crate::persistence::test_dir("sync-alias-raw");
        fs::write(base.join("source"), b"data").unwrap();
        let raw = std::ffi::OsString::from_vec(b"name-\xff".to_vec());
        let literal = OsStr::new("name-\\xff");
        fs::hard_link(base.join("source"), base.join(&raw)).unwrap();
        fs::hard_link(base.join("source"), base.join(literal)).unwrap();
        let parent = File::open(&base).unwrap();
        let cancel = AtomicBool::new(false);
        let mut source = capture_for_sync(&parent, OsStr::new("source"), 4, &cancel).unwrap();
        let mut a = capture_for_sync(&parent, &raw, 4, &cancel).unwrap();
        let mut b = capture_for_sync(&parent, literal, 4, &cancel).unwrap();
        fs::rename(base.join("source"), base.join("renamed")).unwrap();
        let change = source
            .reconcile_root_rename(&parent, OsStr::new("renamed"), 4, &cancel)
            .unwrap();
        assert_eq!(
            change
                .reconcile_aliases(
                    &mut [
                        AliasScope {
                            inventory: &mut a,
                            parent: &parent,
                            name: &raw
                        },
                        AliasScope {
                            inventory: &mut b,
                            parent: &parent,
                            name: literal
                        }
                    ],
                    8,
                    &cancel
                )
                .unwrap()
                .occurrences,
            2
        );
        a.verify(&parent, &raw, &cancel).unwrap();
        b.verify(&parent, literal, &cancel).unwrap();
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn alias_batch_symlink_inode_transition_uses_captured_atime_policy() {
        use std::os::unix::fs::symlink;
        let base = crate::persistence::test_dir("sync-alias-link");
        symlink("literal-target", base.join("source")).unwrap();
        fs::hard_link(base.join("source"), base.join("a")).unwrap();
        fs::hard_link(base.join("source"), base.join("b")).unwrap();
        // Warm readlink before all captures so relatime doesn't give separate
        // scopes different original access values. Actual atime-only changes
        // after capture remain excluded by the documented symlink policy.
        fs::read_link(base.join("source")).unwrap();
        let parent = File::open(&base).unwrap();
        let cancel = AtomicBool::new(false);
        let mut source = capture_for_sync(&parent, OsStr::new("source"), 0, &cancel).unwrap();
        let mut a = capture_for_sync(&parent, OsStr::new("a"), 0, &cancel).unwrap();
        let mut b = capture_for_sync(&parent, OsStr::new("b"), 0, &cancel).unwrap();
        fs::rename(base.join("source"), base.join("renamed")).unwrap();
        let change = source
            .reconcile_root_rename(&parent, OsStr::new("renamed"), 0, &cancel)
            .unwrap();
        assert_eq!(
            change
                .reconcile_aliases(
                    &mut [
                        AliasScope {
                            inventory: &mut a,
                            parent: &parent,
                            name: OsStr::new("a")
                        },
                        AliasScope {
                            inventory: &mut b,
                            parent: &parent,
                            name: OsStr::new("b")
                        }
                    ],
                    0,
                    &cancel
                )
                .unwrap(),
            AliasReport {
                occurrences: 2,
                payload_read: 0
            }
        );
        a.verify(&parent, OsStr::new("a"), &cancel).unwrap();
        b.verify(&parent, OsStr::new("b"), &cancel).unwrap();
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn alias_batch_cooperative_pause_resume_and_cancel_preserve_atomicity() {
        use std::{sync::mpsc, time::Duration};
        for cancel in [false, true] {
            let mut f = Fixture::new();
            let one = format!("{:?}", f.one.entries);
            let two = format!("{:?}", f.two.entries);
            let control = Control::default();
            control.pause();
            let worker_control = control.clone();
            let (entered_tx, entered_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            let worker = std::thread::spawn(move || {
                entered_tx.send(()).unwrap();
                let result = f.change.reconcile_aliases_controlled(
                    &mut [
                        AliasScope {
                            inventory: &mut f.one,
                            parent: &f.parent,
                            name: OsStr::new("one"),
                        },
                        AliasScope {
                            inventory: &mut f.two,
                            parent: &f.parent,
                            name: OsStr::new("two"),
                        },
                    ],
                    18,
                    &worker_control,
                );
                done_tx
                    .send((
                        result,
                        format!("{:?}", f.one.entries),
                        format!("{:?}", f.two.entries),
                    ))
                    .unwrap();
            });
            entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            assert!(done_rx.recv_timeout(Duration::from_millis(30)).is_err());
            if cancel {
                control.cancel();
            } else {
                control.resume();
            }
            let (result, after_one, after_two) =
                done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            worker.join().unwrap();
            if cancel {
                assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
                assert_eq!(after_one, one);
                assert_eq!(after_two, two);
            } else {
                assert_eq!(result.unwrap().occurrences, 3);
            }
        }
    }
    #[test]
    fn alias_batch_full_combined_entry_budget_and_one_over_are_not_truncated() {
        let base = crate::persistence::test_dir("sync-alias-full");
        fs::write(base.join("source"), b"data").unwrap();
        let names: Vec<_> = (0..4).map(|i| format!("s{i}")).collect();
        for n in &names {
            fs::create_dir(base.join(n)).unwrap();
            fs::hard_link(base.join("source"), base.join(n).join("alias")).unwrap();
            for i in 0..MAX_ENTRIES - 1 {
                fs::write(base.join(n).join(format!("n{i:05}")), b"").unwrap();
            }
        }
        fs::hard_link(base.join("source"), base.join("extra")).unwrap();
        let parent = File::open(&base).unwrap();
        let cancel = AtomicBool::new(false);
        let mut source = capture_for_sync(&parent, OsStr::new("source"), 4, &cancel).unwrap();
        let mut inventories: Vec<_> = names
            .iter()
            .map(|n| capture_for_sync(&parent, OsStr::new(n), 4, &cancel).unwrap())
            .collect();
        let mut extra = capture_for_sync(&parent, OsStr::new("extra"), 4, &cancel).unwrap();
        assert_eq!(
            inventories.iter().map(|i| i.entries.len()).sum::<usize>(),
            MAX_ALIAS_ENTRIES
        );
        fs::rename(base.join("source"), base.join("renamed")).unwrap();
        let change = source
            .reconcile_root_rename(&parent, OsStr::new("renamed"), 4, &cancel)
            .unwrap();
        let mut scopes: Vec<_> = inventories
            .iter_mut()
            .zip(&names)
            .map(|(inventory, n)| AliasScope {
                inventory,
                parent: &parent,
                name: OsStr::new(n),
            })
            .collect();
        scopes.push(AliasScope {
            inventory: &mut extra,
            parent: &parent,
            name: OsStr::new("extra"),
        });
        assert!(change.reconcile_aliases(&mut scopes, 20, &cancel).is_err());
        scopes.pop();
        assert_eq!(
            change.reconcile_aliases(&mut scopes, 16, &cancel).unwrap(),
            AliasReport {
                occurrences: 4,
                payload_read: 16
            }
        );
        drop(scopes);
        for (inv, n) in inventories.iter().zip(&names) {
            assert_eq!(inv.entries.len(), MAX_ENTRIES + 1);
            inv.verify_for_sync(&parent, OsStr::new(n), 4, &cancel)
                .unwrap();
        }
        assert!(extra.verify(&parent, OsStr::new("extra"), &cancel).is_err());
        fs::remove_dir_all(base).unwrap();
    }
}
