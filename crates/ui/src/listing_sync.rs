//! UI-side admission of bounded directory invalidations. No filesystem calls.
use crate::{
    listing_watch::{ListingWatch, Status, Target},
    model::{Model, View},
};
use std::{
    ffi::OsString,
    time::{Duration, Instant},
};

pub struct Viewport {
    name: OsString,
    offset: i32,
    view: View,
    dual: bool,
}
impl Viewport {
    pub fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.name.capacity()
    }
    pub fn capture(m: &Model) -> Option<Self> {
        if m.scroll <= 0 || m.visible_generation != m.generation {
            return None;
        }
        let i = m.visible.iter().copied().find(|i| m.order.contains(i))?;
        let top = crate::view::entry_top(m, i, m.listing_sync.width)?;
        let name = &m.entries.get(i)?.name;
        if name.len() > 4096 {
            return None;
        }
        Some(Self {
            name: name.clone(),
            offset: top.saturating_sub(m.scroll),
            view: m.view,
            dual: m.dual.enabled,
        })
    }
    pub fn capture_other(b: &crate::dual_pane::Browser) -> Option<Self> {
        if b.scroll <= 0 {
            return None;
        }
        let rank = (b.scroll.max(0) / 36) as usize;
        let i = *b.order.get(rank)?;
        let name = &b.entries.get(i)?.name;
        if name.len() > 4096 {
            return None;
        }
        Some(Self {
            name: name.clone(),
            offset: (rank as i32 * 36).saturating_sub(b.scroll),
            view: View::List,
            dual: true,
        })
    }
    pub fn restore(self, m: &mut Model) {
        if self.view != m.view || self.dual != m.dual.enabled {
            return;
        }
        if let Some(i) = m
            .order
            .iter()
            .copied()
            .find(|&i| m.entries[i].name == self.name)
            && let Some(top) = crate::view::entry_top(m, i, m.listing_sync.width)
        {
            m.scroll = top.saturating_sub(self.offset).max(0);
        }
    }
    pub fn restore_other(self, b: &mut crate::dual_pane::Browser) {
        if let Some(rank) = b.order.iter().position(|&i| b.entries[i].name == self.name) {
            b.scroll = (rank as i32 * 36).saturating_sub(self.offset).max(0);
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner {
    Browser((u64, u64)),
    Column((u64, u64), u64),
}
struct Slot {
    owner: Owner,
    target: Target,
    serial: u64,
    changes: u64,
    attachment: u64,
    first: Option<Instant>,
    quiet: Instant,
    failed: bool,
}
pub struct State {
    watch: Option<ListingWatch>,
    attempted: bool,
    stopping: bool,
    slots: Vec<Slot>,
    next: u64,
    enabled: bool,
    pub notice: String,
    pub width: i32,
    pub refreshes: u64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            watch: None,
            attempted: false,
            stopping: false,
            slots: Vec::new(),
            next: 0,
            enabled: !cfg!(test),
            notice: String::new(),
            width: 1000,
            refreshes: 0,
        }
    }
}
impl State {
    #[cfg(test)]
    pub fn enable(&mut self) {
        self.enabled = true;
    }
    pub fn stop(&mut self) {
        self.stopping = true;
        if let Some(w) = &mut self.watch {
            w.stop();
        }
    }
    pub fn stopped(&self) -> bool {
        self.watch.as_ref().is_none_or(ListingWatch::stopped)
    }
    pub fn pending(&self) -> bool {
        self.watch.as_ref().is_some_and(ListingWatch::pending)
    }
    pub fn settled(&self) -> bool {
        !self.stopping
            && self.watch.is_some()
            && !self.slots.is_empty()
            && !self.pending()
            && self
                .slots
                .iter()
                .all(|s| s.serial > 0 && s.first.is_none() && !s.failed)
    }
    pub fn retry(&mut self) {
        if self.watch.is_none() {
            self.attempted = false;
        }
    }
    pub fn refresh_started(&mut self, owner: (u64, u64), path: &std::path::Path) {
        let Some(w) = &mut self.watch else {
            return;
        };
        for n in w.notices() {
            if let Some(slot) = self.slots.iter_mut().find(|s| {
                s.owner == Owner::Browser(owner) && s.target == n.target && s.target.path == path
            }) {
                slot.failed = n.status == Status::Failed;
                slot.serial = n.serial;
                slot.changes = n.changes;
                slot.attachment = n.attachment;
                slot.first = None;
                w.acknowledge(&n);
            }
        }
        if !self.pending() && self.slots.iter().all(|s| !s.failed) {
            self.notice.clear();
        }
    }
    fn sync(&mut self, m: &Model) {
        let mut desired = Vec::with_capacity(crate::listing_watch::MAX_TARGETS);
        desired.push((Owner::Browser(m.pane_owner()), m.path.as_path()));
        if m.view == View::Columns && !m.dual.enabled {
            desired.extend(
                m.columns
                    .columns
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != m.columns.active)
                    .map(|(_, c)| (Owner::Column(m.pane_owner(), c.watch_id), c.path.as_path())),
            );
        } else if let Some(b) = m.dual.other.as_ref().filter(|_| m.dual.enabled) {
            desired.push((Owner::Browser((b.id, b.tabs.active)), b.path.as_path()));
        }
        let same = self.slots.len() == desired.len()
            && desired.iter().copied().all(|(owner, path)| {
                self.slots
                    .iter()
                    .any(|s| s.owner == owner && s.target.path == path)
            });
        if same {
            return;
        }
        let mut slots = Vec::with_capacity(desired.len());
        let mut old = std::mem::take(&mut self.slots);
        for (owner, path) in desired {
            if let Some(i) = old
                .iter()
                .position(|s| s.owner == owner && s.target.path == path)
            {
                slots.push(old.swap_remove(i));
                continue;
            }
            self.next = self
                .next
                .checked_add(1)
                .expect("listing owner token exhausted");
            slots.push(Slot {
                owner,
                target: Target {
                    key: self.next,
                    path: path.to_path_buf(),
                },
                serial: 0,
                changes: 0,
                attachment: 0,
                first: None,
                quiet: Instant::now(),
                failed: false,
            });
        }
        if let Some(w) = &mut self.watch {
            match w.configure(slots.iter().map(|s| s.target.clone()).collect()) {
                Ok(_) => self.slots = slots,
                Err(_) => {
                    let _ = w.configure(vec![]);
                    self.slots = slots;
                    for s in &mut self.slots {
                        s.failed = true;
                    }
                    self.notice = "Vigilancia no disponible · F5 actualiza".into();
                }
            }
        }
    }
    fn pump(&mut self, m: &mut Model) -> bool {
        let before = self.notice.clone();
        if m.closing {
            self.stop();
            return false;
        }
        if !self.enabled {
            return false;
        }
        if self.stopping {
            if !self.stopped() {
                self.notice = "Vigilancia reactivándose · F5 actualiza".into();
                return self.notice != before;
            }
            self.watch = None;
            self.attempted = false;
            self.slots.clear();
            self.stopping = false;
        }
        if !self.attempted {
            self.attempted = true;
            match ListingWatch::new() {
                Ok(w) => self.watch = Some(w),
                Err(_) => self.notice = "Vigilancia no disponible · F5 actualiza".into(),
            }
        }
        if self.watch.is_none() {
            return self.notice != before;
        }
        self.sync(m);
        let pending = self.pending();
        if !pending {
            return self.notice != before;
        }
        if m.view == View::SpaceMap && !m.dual.enabled {
            let mut dirty = false;
            for n in self.watch.as_ref().unwrap().notices() {
                let Some(slot) = self.slots.iter_mut().find(|s| {
                    s.target == n.target
                        && s.owner == Owner::Browser(m.pane_owner())
                        && n.target.path == m.path
                }) else {
                    continue;
                };
                let gap = n.status == Status::Failed
                    || slot.attachment > 0 && n.attachment != slot.attachment;
                if gap || n.changes != slot.changes {
                    dirty |= m.space_map.invalidate(gap);
                }
                slot.failed = n.status == Status::Failed;
                slot.serial = n.serial;
                slot.changes = n.changes;
                slot.attachment = n.attachment;
                slot.first = None;
                self.watch.as_mut().unwrap().acknowledge(&n);
            }
            self.notice = if self.slots.iter().any(|s| s.failed) {
                "Vigilancia no disponible · F5 recalcula".into()
            } else {
                String::new()
            };
            return dirty || self.notice != before;
        }
        self.notice = "Cambios pendientes · se aplicarán al terminar".into();
        if blocked(m) {
            return self.notice != before;
        }
        let notices = self.watch.as_ref().unwrap().notices();
        let mut refreshed = false;
        for n in notices {
            let Some(slot) = self.slots.iter_mut().find(|s| s.target == n.target) else {
                continue;
            };
            slot.failed = n.status == Status::Failed;
            let now = Instant::now();
            if slot.serial != n.serial {
                slot.serial = n.serial;
                slot.quiet = now;
                slot.first.get_or_insert(now);
            }
            if now.duration_since(slot.quiet) < Duration::from_millis(180)
                && slot
                    .first
                    .is_some_and(|t| now.duration_since(t) < Duration::from_secs(1))
            {
                continue;
            }
            let admitted = if slot.owner == Owner::Browser(m.pane_owner())
                && n.target.path == m.path
                && m.loaded
                && !m.selection_refreshing()
            {
                m.refresh_current();
                !m.refresh_deferred
            } else if let Some(b) = m.dual.other.as_mut().filter(|b| {
                m.dual.enabled
                    && Owner::Browser((b.id, b.tabs.active)) == slot.owner
                    && b.path == n.target.path
                    && b.loaded
                    && b.refresh_selection.is_none()
                    && b.refresh_viewport.is_none()
            }) {
                if m.sizes.root() == b.path {
                    m.sizes.invalidate();
                    m.du_requested = None;
                }
                b.refresh().is_ok()
            } else if let Owner::Column(owner, watch_id) = slot.owner {
                if m.view == View::Columns && !m.dual.enabled && owner == m.pane_owner() {
                    let mut columns = std::mem::take(&mut m.columns);
                    let admitted = columns.refresh_cached(m, watch_id, &n.target.path);
                    m.columns = columns;
                    admitted
                } else {
                    false
                }
            } else {
                false
            };
            if admitted {
                slot.changes = n.changes;
                slot.attachment = n.attachment;
                if let Some(w) = &mut self.watch {
                    w.acknowledge(&n);
                }
                slot.first = None;
                self.refreshes = self
                    .refreshes
                    .checked_add(1)
                    .expect("listing refresh counter exhausted");
                refreshed = true;
            }
        }
        if self.slots.iter().any(|s| s.failed) {
            self.notice = "Vigilancia no disponible · F5 actualiza".into();
        } else if !self.pending() {
            self.notice.clear();
        }
        refreshed || self.notice != before
    }
}
fn blocked(m: &Model) -> bool {
    m.dual_blocked()
        || m.refresh_viewport.is_some()
        || m.view == View::Spatial && !m.layout_ready
        || m.filter_edit.active
        || m.toolkit_popup
        || m.drag.is_some()
        || m.commands.is_some()
        || m.size_panel.is_some()
        || m.checksums.is_some()
        || m.comparison.is_some()
        || m.help.is_some()
        || m.bookmarks.panel.is_some()
        || m.sftp.form.is_some()
        || m.search.visible
        || m.search.busy()
        || m.sizes.has_work()
        || m.du_job.is_some()
        || !m.preview.is_drained()
        || !m.space_map.is_drained()
        || m.view == View::SpaceMap
        || m.copy.active_id().is_some()
        || m.copy.queued_count() > 0
        || m.search_transfer.busy()
        || m.archive_preparing.is_some()
        || m.move_preparing.is_some()
        || m.recovery_catalog.open
        || m.native_transfer.refresh_blocked()
        || !m.clipboard_state.is_drained()
}
impl Model {
    pub fn pump_listing_watch(&mut self) -> bool {
        let mut state = std::mem::take(&mut self.listing_sync);
        // Preserve width while refresh helpers read it from Model.
        self.listing_sync.width = state.width;
        let dirty = state.pump(self);
        self.listing_sync = state;
        dirty
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::{
            atomic::{AtomicU64, Ordering},
            mpsc,
        },
    };
    struct Fixture {
        root: PathBuf,
        m: Model,
    }
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = files_core::theme::home()
                .join(".cache/files-test")
                .join(format!(
                    "listing-model-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            fs::create_dir_all(&root).unwrap();
            for name in ["a.txt", "b.txt", "c.txt"] {
                fs::write(root.join(name), b"owned").unwrap();
            }
            let mut m = Model::new(root.clone());
            m.view = View::List;
            let (tx, _) = mpsc::sync_channel(4);
            let (_, rx) = mpsc::channel();
            m.aux = crate::aux_io::AuxIo { tx, rx };
            m.listing_sync.enable();
            Self { root, m }
        }
        fn wait(&mut self, check: impl Fn(&Model) -> bool) {
            let deadline = Instant::now() + Duration::from_secs(8);
            loop {
                self.m.pump();
                if check(&self.m) {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "{} / {}",
                    self.m.error,
                    self.m.listing_sync.notice
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        fn ready(&mut self) {
            self.wait(|m| {
                m.loaded
                    && !m.selection_refreshing()
                    && m.refresh_viewport.is_none()
                    && m.listing_sync.settled()
                    && (m.view != View::Groups && m.sort == files_core::listing::Sort::Name
                        || m.entries.iter().all(|e| e.stat_attempted))
            });
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            self.m.listing_sync.stop();
            let deadline = Instant::now() + Duration::from_secs(3);
            while !self.m.listing_sync.stopped() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(
                self.m.listing_sync.stopped(),
                "watch must stop before fixture cleanup"
            );
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    #[test]
    fn external_changes_preserve_marks_focus_range_anchor_and_named_viewport() {
        use std::os::unix::ffi::OsStringExt;
        let mut f = Fixture::new();
        let raw = OsString::from_vec(b"raw-\xff.txt".to_vec());
        fs::write(f.root.join(&raw), b"raw").unwrap();
        f.ready();
        let a = f.m.entries.iter().position(|e| e.name == "b.txt").unwrap();
        let b = f.m.entries.iter().position(|e| e.name == raw).unwrap();
        f.m.selection = std::collections::HashSet::from([a, b]);
        f.m.selected = Some(b);
        f.m.anchor = Some(a);
        f.m.visible = vec![a, b];
        f.m.visible_generation = f.m.generation;
        f.m.scroll = crate::view::entry_top(&f.m, a, 1000).unwrap() + 7;
        let offset = crate::view::entry_top(&f.m, a, 1000).unwrap() - f.m.scroll;
        fs::write(f.root.join("0-new.txt"), b"new").unwrap();
        f.wait(|m| {
            m.loaded && m.entries.iter().any(|e| e.name == "0-new.txt") && m.listing_sync.settled()
        });
        assert_eq!(f.m.entries[f.m.selected.unwrap()].name, raw);
        assert_eq!(f.m.entries[f.m.anchor.unwrap()].name, "b.txt");
        assert_eq!(f.m.selection.len(), 2);
        let a = f.m.entries.iter().position(|e| e.name == "b.txt").unwrap();
        assert_eq!(
            crate::view::entry_top(&f.m, a, 1000).unwrap() - f.m.scroll,
            offset
        );
        f.m.selection.clear();
        f.m.selected = None;
        f.m.anchor = None;
        f.m.preserve_empty_selection = true;
        fs::remove_file(f.root.join("c.txt")).unwrap();
        f.wait(|m| {
            m.loaded && !m.entries.iter().any(|e| e.name == "c.txt") && m.listing_sync.settled()
        });
        assert!(f.m.selection.is_empty() && f.m.selected.is_none());
    }
    #[test]
    fn edits_and_paused_size_queue_defer_without_losing_dirty_notifications() {
        let mut f = Fixture::new();
        f.ready();
        let generation = f.m.generation;
        f.m.editing = true;
        fs::write(f.root.join("during-edit.txt"), b"new").unwrap();
        f.wait(|m| m.listing_sync.pending());
        assert_eq!(f.m.generation, generation);
        assert!(!f.m.entries.iter().any(|e| e.name == "during-edit.txt"));
        f.m.editing = false;
        f.ready();
        assert!(f.m.entries.iter().any(|e| e.name == "during-edit.txt"));
        fs::create_dir(f.root.join("size-dir")).unwrap();
        f.ready();
        f.m.sizes
            .enqueue(&f.root, [std::ffi::OsStr::new("size-dir")].into_iter())
            .unwrap();
        f.m.sizes.pause(true);
        let id = f.m.sizes.rows(&f.root)[0].id;
        let generation = f.m.generation;
        fs::write(f.root.join("during-size.txt"), b"new").unwrap();
        f.wait(|m| m.listing_sync.pending());
        assert_eq!(f.m.generation, generation);
        assert_eq!(f.m.sizes.rows(&f.root)[0].id, id);
        assert!(f.m.sizes.paused());
        f.m.sizes.cancel_all();
        f.ready();
        assert!(f.m.entries.iter().any(|e| e.name == "during-size.txt"));
    }
    #[test]
    fn grid_and_group_viewports_restore_by_name_after_required_metadata() {
        for view in [View::Grid, View::Groups] {
            let mut f = Fixture::new();
            for i in 0..60 {
                fs::write(f.root.join(format!("zz{i:02}.txt")), b"owned").unwrap();
            }
            f.m.view = view;
            f.m.sort = files_core::listing::Sort::Name;
            f.ready();
            let i =
                f.m.entries
                    .iter()
                    .position(|e| e.name == "zz30.txt")
                    .unwrap();
            f.m.visible = vec![i];
            f.m.visible_generation = f.m.generation;
            f.m.scroll = crate::view::entry_top(&f.m, i, 1000).unwrap() + 9;
            let offset = crate::view::entry_top(&f.m, i, 1000).unwrap() - f.m.scroll;
            for n in 0..10 {
                fs::create_dir(f.root.join(format!("new-folder-{n}"))).unwrap();
            }
            f.wait(|m| {
                m.loaded
                    && m.entries.iter().filter(|e| e.is_dir()).count() == 10
                    && m.refresh_viewport.is_none()
                    && m.listing_sync.settled()
            });
            let i =
                f.m.entries
                    .iter()
                    .position(|e| e.name == "zz30.txt")
                    .unwrap();
            assert_eq!(
                crate::view::entry_top(&f.m, i, 1000).unwrap() - f.m.scroll,
                offset
            );
        }
    }
    #[test]
    fn restored_tab_reconciles_external_changes_and_watch_restarts_after_close_cancel() {
        let mut f = Fixture::new();
        f.ready();
        let original = f.m.tabs.active;
        let elsewhere = f.root.join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        f.m.tab_command(crate::tabs::Command::New);
        f.m.navigate(elsewhere);
        f.ready();
        assert_ne!(f.m.tabs.active, original);
        fs::write(f.root.join("while-tab-away.txt"), b"owned").unwrap();
        f.m.tab_command(crate::tabs::Command::Select(original));
        f.wait(|m| {
            m.loaded
                && m.entries.iter().any(|e| e.name == "while-tab-away.txt")
                && m.listing_sync.settled()
        });
        f.m.listing_sync.stop();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !f.m.listing_sync.stopped() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        f.ready();
        fs::write(f.root.join("after-resume.txt"), b"owned").unwrap();
        f.wait(|m| {
            m.loaded
                && m.entries.iter().any(|e| e.name == "after-resume.txt")
                && m.listing_sync.settled()
        });
    }
    #[test]
    fn inactive_pane_updates_without_switching_focus_or_rearming_after_refresh() {
        let mut f = Fixture::new();
        f.ready();
        let other = f.root.join("other");
        fs::create_dir(&other).unwrap();
        f.m.toggle_dual();
        f.m.navigate(other.clone());
        f.wait(|m| {
            m.loaded && m.dual.other.as_ref().is_some_and(|b| b.loaded) && m.listing_sync.settled()
        });
        let owner = f.m.pane_owner();
        let refreshes = f.m.listing_sync.refreshes;
        fs::write(f.root.join("inactive.txt"), b"new").unwrap();
        f.wait(|m| {
            m.dual
                .other
                .as_ref()
                .is_some_and(|b| b.loaded && b.entries.iter().any(|e| e.name == "inactive.txt"))
                && m.listing_sync.settled()
        });
        assert_eq!(f.m.pane_owner(), owner);
        assert_eq!(f.m.path, other);
        assert!(f.m.listing_sync.refreshes > refreshes);
        let keys =
            f.m.listing_sync
                .slots
                .iter()
                .map(|s| s.target.key)
                .collect::<Vec<_>>();
        f.m.refresh_current();
        f.ready();
        assert_eq!(
            keys,
            f.m.listing_sync
                .slots
                .iter()
                .map(|s| s.target.key)
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn space_map_consumes_hints_without_listing_reload_and_explicit_refresh_renews_mapping() {
        let mut f = Fixture::new();
        // Prepara antes de vigilar la raíz sin recursión. Renombrar publica un
        // evento; escribir genera Create + Modify, con callbacks que pueden
        // llegar a ambos lados del recálculo explícito. Un callback tardío debe
        // conservar el aviso de cambios; no se suprime para hacer pasar el test.
        let staging = f.root.join("staging");
        fs::create_dir(&staging).unwrap();
        fs::write(staging.join("next"), b"changed").unwrap();
        f.ready();
        f.m.view = View::SpaceMap;
        f.m.space_requested = false;
        let generation = f.m.generation;
        let refreshes = f.m.listing_sync.refreshes;
        fs::rename(staging.join("next"), f.root.join("new-child.txt")).unwrap();
        f.wait(|m| m.space_map.changed() && m.listing_sync.settled());
        assert_eq!(f.m.generation, generation);
        assert_eq!(f.m.listing_sync.refreshes, refreshes);
        assert!(f.m.space_map.is_drained());
        assert!(!f.m.space_requested);
        assert!(!f.m.entries.iter().any(|e| e.name == "new-child.txt"));
        f.m.action(crate::model::Action::SpaceRefresh);
        assert!(f.m.space_requested);
        // Unit-test executables live in deps/, without a sibling files-helper.
        // The owned release UI fixture verifies dispatch and the resulting bytes.
        f.m.space_requested = false;
        f.wait(|m| m.loaded && m.listing_sync.settled());
        assert!(f.m.entries.iter().any(|e| e.name == "new-child.txt"));
        assert!(!f.m.space_map.changed() && !f.m.space_map.watch_gap());
        let generation = f.m.generation;
        fs::write(f.root.join("new-child.txt"), b"later change").unwrap();
        f.wait(|m| m.space_map.changed() && m.listing_sync.settled());
        assert_eq!(f.m.generation, generation);
        assert!(!f.m.space_requested);
    }
}
