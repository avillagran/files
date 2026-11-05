//! One foreground Model and a movable second browser, shared operation/media owners.
use crate::{
    model::{Action, Model, View},
    worker::{Job, Update, Worker},
};
use files_core::{
    listing::{Entry, Sort},
    natural_sort::NaturalKeyRef,
};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};
pub const DATA_LIMIT: usize = 32 * 1024 * 1024;
pub struct Browser {
    pub id: u64,
    pub revision: u64,
    pub worker: Option<Worker>,
    pub path: PathBuf,
    pub history: files_core::navigation_history::History,
    pub tabs: crate::tabs::Tabs,
    pub entries: Vec<Entry>,
    pub order: Vec<usize>,
    pub selected: Option<usize>,
    pub selection: HashSet<usize>,
    pub anchor: Option<usize>,
    pub preserve_empty_selection: bool,
    pub sort: Sort,
    pub descending: bool,
    pub query: String,
    pub scroll: i32,
    pub content_height: i32,
    pub loaded: bool,
    pub error: String,
    pub free: Option<u64>,
    pub refresh_selection: Option<crate::refresh_selection::Snapshot>,
    pub refresh_deferred: bool,
    pub refresh_viewport: Option<crate::listing_sync::Viewport>,
    pub summaries: HashMap<usize, crate::folder_summary::Summary>,
    pub requested: HashSet<usize>,
    pub stats_requested: HashSet<usize>,
    pub all_stat: bool,
}
impl Browser {
    fn new(id: u64, path: PathBuf, worker: Option<Worker>) -> Self {
        Self {
            id,
            revision: 1,
            worker,
            path: path.clone(),
            history: Default::default(),
            tabs: crate::tabs::Tabs::new(path.clone()),
            entries: Vec::new(),
            order: Vec::new(),
            selected: None,
            selection: HashSet::new(),
            anchor: None,
            preserve_empty_selection: false,
            sort: Sort::Name,
            descending: false,
            query: String::new(),
            scroll: 0,
            content_height: 0,
            loaded: false,
            error: String::new(),
            free: None,
            refresh_selection: None,
            refresh_deferred: false,
            refresh_viewport: None,
            summaries: HashMap::new(),
            requested: HashSet::new(),
            stats_requested: HashSet::new(),
            all_stat: false,
        }
    }
    fn swap(&mut self, m: &mut Model) {
        m.end_filter_edit();
        std::mem::swap(&mut self.revision, &mut m.pane_revision);
        std::mem::swap(&mut self.path, &mut m.path);
        std::mem::swap(&mut self.history, &mut m.history);
        std::mem::swap(&mut self.tabs, &mut m.tabs);
        std::mem::swap(&mut self.entries, &mut m.entries);
        std::mem::swap(&mut self.order, &mut m.order);
        std::mem::swap(&mut self.selected, &mut m.selected);
        std::mem::swap(&mut self.selection, &mut m.selection);
        std::mem::swap(&mut self.anchor, &mut m.anchor);
        std::mem::swap(
            &mut self.preserve_empty_selection,
            &mut m.preserve_empty_selection,
        );
        std::mem::swap(&mut self.sort, &mut m.sort);
        std::mem::swap(&mut self.descending, &mut m.descending);
        std::mem::swap(&mut self.query, &mut m.query);
        std::mem::swap(&mut self.scroll, &mut m.scroll);
        std::mem::swap(&mut self.content_height, &mut m.content_height);
        std::mem::swap(&mut self.loaded, &mut m.loaded);
        std::mem::swap(&mut self.error, &mut m.error);
        std::mem::swap(&mut self.free, &mut m.free);
        std::mem::swap(&mut self.refresh_selection, &mut m.refresh_selection);
        std::mem::swap(&mut self.refresh_deferred, &mut m.refresh_deferred);
        std::mem::swap(&mut self.refresh_viewport, &mut m.refresh_viewport);
        std::mem::swap(&mut self.summaries, &mut m.summaries);
        std::mem::swap(&mut self.requested, &mut m.requested);
        std::mem::swap(&mut self.stats_requested, &mut m.stats_requested);
        std::mem::swap(&mut self.all_stat, &mut m.all_stat);
        std::mem::swap(
            self.worker
                .as_mut()
                .expect("visible pane owns listing worker"),
            &mut m.worker,
        );
    }
    pub fn charge(&self) -> usize {
        crate::columns::charge(&self.entries, &self.order, &self.selection)
            + self.query.capacity()
            + self.error.capacity()
            + self.stats_requested.capacity() * 24
            + self.requested.capacity() * 24
            + self.summaries.capacity() * 64
            + self
                .refresh_selection
                .as_ref()
                .map_or(0, |s| s.retained_bytes())
            + self
                .refresh_viewport
                .as_ref()
                .map_or(0, |v| v.retained_bytes())
            + 4096
    }
    fn metadata_charge(&self) -> Result<usize, &'static str> {
        let selection = if let Some(s) = &self.refresh_selection {
            s.retained_bytes()
        } else {
            crate::refresh_selection::Snapshot::estimate(
                self.entries
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| self.selection.contains(i))
                    .map(|(_, e)| e.name.as_os_str()),
                self.selected
                    .and_then(|i| self.entries.get(i))
                    .map(|e| e.name.as_os_str()),
                self.anchor
                    .and_then(|i| self.entries.get(i))
                    .map(|e| e.name.as_os_str()),
                crate::tabs::SELECTION_LIMIT,
            )?
        };
        Ok(selection
            + self
                .refresh_viewport
                .as_ref()
                .map_or(0, |v| v.retained_bytes())
            + self.history.retained_bytes()
            + self.query.capacity()
            + self.path.capacity()
            + 8192)
    }
    fn rebuild(&mut self, hidden: bool, sizes: &crate::size_queue::State) {
        let filter = files_core::name_filter::Filter::new(&self.query);
        self.order.clear();
        self.order.reserve_exact(self.entries.len());
        self.order.extend(
            self.entries
                .iter()
                .enumerate()
                .filter(|(_, e)| {
                    files_core::visibility::accepts(&e.name, hidden) && filter.score(e).is_some()
                })
                .map(|(i, _)| i),
        );
        self.order
            .sort_by_cached_key(|&i| NaturalKeyRef::new(&self.entries[i].name));
        if self.sort == Sort::Size {
            let measured = sizes.measured_map(&self.path);
            self.order.sort_by(|&a, &b| {
                let value = |e: &Entry| {
                    if e.is_dir() {
                        measured.get(e.name.as_os_str()).copied()
                    } else {
                        e.metadata.as_ref().map(|m| m.size)
                    }
                };
                files_core::listing::compare_sizes(
                    value(&self.entries[a]),
                    value(&self.entries[b]),
                    self.descending,
                )
            });
        } else {
            if self.sort == Sort::Type {
                self.order.sort_by(|&a, &b| {
                    files_core::listing::type_key(&self.entries[a].name)
                        .cmp(&files_core::listing::type_key(&self.entries[b].name))
                });
            }
            if self.sort == Sort::Modified {
                self.order.sort_by(|&a, &b| {
                    self.entries[a]
                        .metadata
                        .as_ref()
                        .map(|m| m.modified)
                        .cmp(&self.entries[b].metadata.as_ref().map(|m| m.modified))
                });
            }
            if self.descending {
                self.order.reverse();
            }
        }
        self.order.sort_by_key(|&i| !self.entries[i].is_dir());
        let allowed = |i: &usize| {
            self.entries.get(*i).is_some_and(|e| {
                files_core::visibility::accepts(&e.name, hidden) && filter.score(e).is_some()
            })
        };
        self.selection.retain(allowed);
        self.selected = self.selected.filter(allowed);
        self.anchor = self.anchor.filter(allowed);
        if self.loaded
            && let Some(snapshot) = self.refresh_selection.take()
        {
            self.selection.clear();
            self.selected = None;
            self.anchor = None;
            for (i, e) in self.entries.iter().enumerate().filter(|(_, e)| {
                files_core::visibility::accepts(&e.name, hidden) && filter.score(e).is_some()
            }) {
                if snapshot.contains(&e.name) {
                    self.selection.insert(i);
                }
                if snapshot.focused(&e.name) {
                    self.selected = Some(i);
                }
                if snapshot.anchored(&e.name) {
                    self.anchor = Some(i);
                }
            }
            self.preserve_empty_selection = self.selection.is_empty() && self.selected.is_none();
        } else if self.refresh_selection.is_some() {
            self.selection.clear();
            self.selected = None;
            self.anchor = None;
        }
        if self.loaded
            && (self.sort == Sort::Name
                || self
                    .entries
                    .iter()
                    .filter(|e| files_core::visibility::accepts(&e.name, hidden))
                    .all(|e| e.stat_attempted))
            && let Some(viewport) = self.refresh_viewport.take()
        {
            viewport.restore_other(self);
        }
    }
    fn snapshot(&mut self) -> Result<(), &'static str> {
        if self.refresh_selection.is_none() {
            let viewport = crate::listing_sync::Viewport::capture_other(self);
            let snapshot = crate::refresh_selection::Snapshot::capture_with_anchor(
                self.entries
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| self.selection.contains(i))
                    .map(|(_, e)| e.name.as_os_str()),
                self.selected
                    .and_then(|i| self.entries.get(i))
                    .map(|e| e.name.as_os_str()),
                self.anchor
                    .and_then(|i| self.entries.get(i))
                    .map(|e| e.name.as_os_str()),
                crate::tabs::SELECTION_LIMIT,
            )?;
            self.refresh_selection = Some(snapshot);
            self.refresh_viewport = viewport;
        }
        Ok(())
    }
    fn release_listing(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.entries = Vec::new();
        self.order = Vec::new();
        self.selection = HashSet::new();
        self.selected = None;
        self.anchor = None;
        self.stats_requested = HashSet::new();
        self.requested = HashSet::new();
        self.summaries = HashMap::new();
        self.loaded = false;
        self.all_stat = false;
        self.free = None;
    }
    pub(crate) fn refresh(&mut self) -> Result<(), &'static str> {
        self.snapshot()?;
        self.release_listing();
        self.error.clear();
        if let Some(w) = &mut self.worker {
            w.restart(self.path.clone());
        }
        Ok(())
    }
    fn suspend(&mut self) -> Result<(), &'static str> {
        self.snapshot()?;
        self.release_listing();
        if let Some(w) = &mut self.worker {
            w.suspend();
        }
        Ok(())
    }
    pub fn request_stats(&mut self, indices: &[usize]) {
        let names = indices
            .iter()
            .copied()
            .filter(|i| {
                self.entries.get(*i).is_some_and(|e| !e.stat_attempted)
                    && !self.stats_requested.contains(i)
            })
            .take(64)
            .map(|i| (i, self.entries[i].name.clone()))
            .collect::<Vec<_>>();
        if names.is_empty() {
            return;
        }
        let ids = names.iter().map(|(i, _)| *i).collect::<Vec<_>>();
        if self
            .worker
            .as_ref()
            .is_some_and(|w| w.tx.try_send(Job::Stat(names)).is_ok())
        {
            self.stats_requested.extend(ids);
        }
    }
}
pub struct State {
    pub enabled: bool,
    pub right_active: bool,
    pub active_id: u64,
    pub other: Option<Browser>,
    next: u64,
    hidden: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            enabled: false,
            right_active: false,
            active_id: 1,
            other: None,
            next: 2,
            hidden: false,
        }
    }
}
impl State {
    fn fresh(&mut self) -> u64 {
        let id = self.next;
        self.next = self.next.checked_add(1).expect("panel ID exhausted");
        id
    }
    pub fn other_stored(&self) -> usize {
        self.other.as_ref().map_or(0, |b| {
            b.tabs
                .stored_bytes()
                .saturating_add(b.metadata_charge().unwrap_or(usize::MAX))
        })
    }
    fn metadata_charge(m: &Model) -> Result<usize, &'static str> {
        let selection = if let Some(s) = &m.refresh_selection {
            s.retained_bytes()
        } else {
            crate::refresh_selection::Snapshot::estimate(
                m.entries
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| m.selection.contains(i))
                    .map(|(_, e)| e.name.as_os_str()),
                m.selected
                    .and_then(|i| m.entries.get(i))
                    .map(|e| e.name.as_os_str()),
                m.anchor
                    .and_then(|i| m.entries.get(i))
                    .map(|e| e.name.as_os_str()),
                crate::tabs::SELECTION_LIMIT,
            )?
        };
        Ok(selection
            + m.refresh_viewport
                .as_ref()
                .map_or(0, |v| v.retained_bytes())
            + m.history.retained_bytes()
            + m.query.capacity()
            + m.path.capacity()
            + 8192)
    }
    pub fn data_charge(m: &Model) -> usize {
        crate::columns::charge(&m.entries, &m.order, &m.selection)
            + m.query.capacity()
            + m.error.capacity()
            + m.stats_requested.capacity() * 24
            + m.requested.capacity() * 24
            + m.summaries.capacity() * 64
            + m.refresh_selection
                .as_ref()
                .map_or(0, |s| s.retained_bytes())
            + m.refresh_viewport
                .as_ref()
                .map_or(0, |v| v.retained_bytes())
            + 4096
    }
    pub fn admit(&self, m: &Model, batch: &[Entry], foreground: bool) -> bool {
        let Some(b) = self.other.as_ref() else {
            return true;
        };
        let entries = if foreground { &m.entries } else { &b.entries };
        let count = entries.len().saturating_add(batch.len());
        if count > 100_000 {
            return false;
        }
        if entries
            .iter()
            .chain(batch)
            .try_fold(0usize, |n, e| n.checked_add(e.name.len()))
            .is_none_or(|n| n > 8 * 1024 * 1024)
        {
            return false;
        }
        let growth = count.saturating_sub(entries.capacity()) * std::mem::size_of::<Entry>();
        let future = (m.entries.len() + b.entries.len() + batch.len()) * 64;
        b.charge()
            .saturating_add(Self::data_charge(m))
            .saturating_add(growth)
            .saturating_add(batch.iter().map(|e| e.name.capacity() + 16).sum::<usize>())
            .saturating_add(future)
            <= DATA_LIMIT
    }
    fn toggle(&mut self, m: &mut Model) -> Result<(), &'static str> {
        let other = if self.other.is_none() {
            128 * 1024
        } else {
            self.other_stored()
        };
        if m.tabs.stored_bytes().saturating_add(other) > crate::tabs::STORED_LIMIT {
            return Err(
                "Paneles: presupuesto conjunto de estado agotado; reduce selección o cierra pestañas",
            );
        }

        if self.enabled {
            if let Some(b) = &mut self.other {
                b.suspend()?;
            }
            self.enabled = false;
            self.right_active = false;
        } else {
            if Self::data_charge(m) > DATA_LIMIT {
                return Err(
                    "Listado actual supera presupuesto de doble panel; usa una carpeta menor",
                );
            }
            m.columns.clear();
            let spare = m.columns.worker.take();
            if let Some(other) = self.other.as_mut() {
                if let Some(spare) = spare {
                    if other.worker.is_some() {
                        m.columns.worker = Some(spare);
                        return Err(
                            "Propiedad del listado secundario duplicada; cambio de modo rechazado",
                        );
                    }
                    other.worker = Some(spare);
                }
            } else {
                let id = self.fresh();
                self.other = Some(Browser::new(id, m.path.clone(), spare));
            }
            let b = self.other.as_mut().expect("other");
            if let Some(worker) = b.worker.as_mut() {
                worker.restart(b.path.clone());
            } else {
                b.worker = Some(Worker::start(b.path.clone()));
            }
            self.enabled = true;
            self.hidden = m.show_hidden;
            m.view = View::List;
            m.scroll = 0;
            m.rebuild();
        }
        m.context_menu = None;
        m.hits.clear();
        m.visible.clear();
        m.preview.close();
        m.thumbnails.suspend();
        m.reset_space_map();
        Ok(())
    }
    fn switch(&mut self, m: &mut Model) -> Result<(), &'static str> {
        if !self.enabled {
            return Ok(());
        }
        let outgoing = Self::metadata_charge(m)?;
        let b = self.other.as_mut().expect("other");
        if m.tabs
            .stored_bytes()
            .saturating_add(b.tabs.stored_bytes())
            .saturating_add(outgoing)
            > crate::tabs::STORED_LIMIT
        {
            return Err("Paneles: estado conjunto supera3MiB; reduce selección o cierra pestañas");
        }
        b.swap(m);
        std::mem::swap(&mut self.active_id, &mut b.id);
        self.right_active = !self.right_active;
        m.generation = m.generation.wrapping_add(1);
        m.visible_generation = 0;
        m.visible.clear();
        m.hits.clear();
        m.hovered = None;
        m.columns.clear();
        m.preview.close();
        m.thumbnails.suspend();
        m.sizes.invalidate();
        if m.sort == Sort::Size {
            m.rebuild();
        }
        if b.sort == Sort::Size {
            b.rebuild(m.show_hidden, &m.sizes);
        }
        m.du_requested = None;
        m.du_target = None;
        m.du_text = None;
        if let Some(j) = &m.du_job {
            j.cancel();
        }
        m.reset_space_map();
        m.layout = Default::default();
        m.layout_path = None;
        m.layout_ready = false;
        m.layout_requested = false;
        m.layout_dirty = false;
        m.drag = None;
        m.session.restore_allowed = false;
        Ok(())
    }
    pub fn pump(&mut self, m: &mut Model) -> bool {
        if !self.enabled {
            return false;
        }
        let mut dirty = false;
        if self.hidden != m.show_hidden {
            self.hidden = m.show_hidden;
            if let Some(b) = &mut self.other {
                b.rebuild(m.show_hidden, &m.sizes);
            }
            dirty = true;
        }
        for _ in 0..32 {
            let Some(update) = self
                .other
                .as_ref()
                .and_then(|b| b.worker.as_ref())
                .and_then(|w| w.rx.try_recv().ok())
            else {
                break;
            };
            dirty = true;
            let admitted = match &update {
                Update::Entries(batch) => self.admit(m, batch, false),
                _ => true,
            };
            let b = self.other.as_mut().expect("other");
            match update {
                Update::Entries(batch) => {
                    if admitted {
                        b.entries.reserve_exact(batch.len());
                        b.entries.extend(batch);
                    } else {
                        b.error = "Listado parcial: presupuesto conjunto agotado".into();
                        b.loaded = true;
                        if let Some(w) = &mut b.worker {
                            w.suspend();
                        }
                    }
                }
                Update::Done => b.loaded = true,
                Update::Error(e) => {
                    b.error = e;
                    b.loaded = true;
                }
                Update::FreeSpace(free) => b.free = free,
                Update::Waiting(true) => b.error = crate::worker::CAPACITY_MESSAGE.into(),
                Update::Waiting(false) => {
                    if b.error == crate::worker::CAPACITY_MESSAGE {
                        b.error.clear();
                    }
                }
                Update::Summary(index, summary) => {
                    if b.entries.get(index).is_some_and(Entry::is_dir) {
                        b.summaries.insert(index, summary);
                    }
                }
                Update::Stats(stats) => {
                    for (i, metadata) in stats {
                        b.stats_requested.remove(&i);
                        if let Some(e) = b.entries.get_mut(i) {
                            e.metadata = metadata;
                            e.stat_attempted = true;
                        }
                    }
                }
                _ => {}
            }
            b.rebuild(m.show_hidden, &m.sizes);
        }
        dirty
    }
    pub fn begin_close(&mut self) {
        if let Some(b) = &mut self.other {
            if let Some(w) = &mut b.worker {
                w.suspend();
            }
            b.release_listing();
        }
        self.enabled = false;
    }
    pub fn refresh_other(&mut self, paths: impl Iterator<Item = PathBuf>) {
        if !self.enabled {
            return;
        }
        if let Some(b) = &mut self.other
            && paths.into_iter().any(|p| b.path.starts_with(p))
            && let Err(e) = b.refresh()
        {
            b.error = e.into();
        }
    }
}
impl Model {
    pub fn dual_blocked(&self) -> bool {
        self.comparison.is_some()
            || self.closing
            || self.editing
            || self.mutations.busy
            || self.mutation_applied
            || self.rename_dialog.is_some()
            || self.properties_dialog.is_some()
            || self.copy_form.is_some()
            || self.notice_open
            || self.preview.visible()
            || self.settings
            || self.actions_menu
            || self.context_menu.is_some()
    }
    pub fn filter_pane(&mut self, id: u64, revision: u64, clear: bool) {
        if !self.dual.enabled || self.closing {
            return;
        }
        let active = id == self.dual.active_id;
        let valid = if active {
            revision == self.pane_revision && self.loaded && !self.selection_refreshing()
        } else {
            self.dual.other.as_ref().is_some_and(|b| {
                b.id == id && b.revision == revision && b.loaded && b.refresh_selection.is_none()
            })
        };
        if !valid {
            return;
        }
        if clear {
            if active {
                self.action(Action::ClearFilter);
            } else if let Some(other) = self.dual.other.as_mut() {
                other.query.clear();
                other.scroll = 0;
                other.rebuild(self.show_hidden, &self.sizes);
                self.session.restore_allowed = false;
            }
        } else {
            if !active {
                self.switch_dual();
            }
            self.action(Action::Filter);
        }
    }
    /// Header sorting never changes the active owner or invalidates shared DU observations.
    pub fn sort_pane(&mut self, id: u64, revision: u64, sort: Sort) {
        if !self.dual.enabled || self.closing {
            return;
        }
        if id == self.dual.active_id {
            if revision == self.pane_revision && !self.selection_refreshing() {
                self.action(Action::Sort(sort));
            }
        } else if let Some(other) = self.dual.other.as_mut()
            && other.id == id
            && other.revision == revision
            && other.loaded
            && other.refresh_selection.is_none()
        {
            if other.sort == sort {
                other.descending = !other.descending;
            } else {
                other.sort = sort;
                other.descending = matches!(sort, Sort::Size | Sort::Modified);
            }
            other.rebuild(self.show_hidden, &self.sizes);
        }
    }
    pub fn rebuild_size_orders(&mut self) {
        if self.sort == Sort::Size {
            self.rebuild();
        }
        self.rebuild_inactive_size_order();
    }
    pub fn rebuild_inactive_size_order(&mut self) {
        if let Some(other) = self.dual.other.as_mut()
            && other.sort == Sort::Size
        {
            other.rebuild(self.show_hidden, &self.sizes);
        }
    }
    pub fn toggle_dual(&mut self) {
        if self.dual_blocked() {
            self.error = "Termina o cancela la edición antes de cambiar paneles".into();
            return;
        }
        let mut s = std::mem::take(&mut self.dual);
        if let Err(e) = s.toggle(self) {
            self.error = e.into();
        }
        self.dual = s;
        self.session.restore_allowed = false;
    }
    pub fn switch_dual(&mut self) {
        if self.dual_blocked() {
            return;
        }
        let mut s = std::mem::take(&mut self.dual);
        if let Err(e) = s.switch(self) {
            self.error = e.into();
        }
        self.dual = s;
    }
    pub fn pump_dual(&mut self) -> bool {
        let mut s = std::mem::take(&mut self.dual);
        let dirty = s.pump(self);
        self.dual = s;
        dirty
    }
    pub fn pane_owner(&self) -> (u64, u64) {
        (self.dual.active_id, self.tabs.active)
    }
    pub fn opposite_destination(&self) -> Option<&Path> {
        self.dual.enabled.then_some(())?;
        self.dual.other.as_ref().map(|b| b.path.as_path())
    }
}
impl Model {
    pub fn pane_select(&mut self, id: u64, rev: u64, i: usize, toggle: bool, extend: bool) {
        if !self.dual.enabled {
            return;
        }
        if id == self.dual.active_id && rev != self.pane_revision {
            return;
        }
        if id != self.dual.active_id {
            if self
                .dual
                .other
                .as_ref()
                .is_none_or(|b| b.id != id || b.revision != rev || !b.order.contains(&i))
            {
                return;
            }
            self.switch_dual();
        }
        if id == self.dual.active_id {
            self.select_entry(i, toggle, extend);
        }
    }
    pub fn pane_background(&mut self, id: u64, rev: u64) -> bool {
        if self.dual_blocked() {
            return false;
        }
        if id == self.dual.active_id {
            if rev != self.pane_revision {
                return false;
            }
        } else {
            if self
                .dual
                .other
                .as_ref()
                .is_none_or(|b| b.id != id || b.revision != rev)
            {
                return false;
            }
            self.switch_dual();
        }
        if id != self.dual.active_id {
            return false;
        }
        self.selection.clear();
        self.selected = None;
        self.anchor = None;
        self.preserve_empty_selection = true;
        self.preview.close();
        true
    }
    pub fn close_focused_pane(&mut self) {
        if !self.dual.enabled || self.dual_blocked() {
            return;
        }
        let closing_id = self.dual.active_id;
        self.switch_dual();
        if self.dual.active_id == closing_id {
            return;
        }
        let mut state = std::mem::take(&mut self.dual);
        if let Some(mut closed) = state.other.take() {
            if let Some(worker) = &mut closed.worker {
                worker.suspend();
            }
            let worker = closed.worker.take();
            let id = state.fresh();
            state.other = Some(Browser::new(id, self.path.clone(), worker));
        }
        state.enabled = false;
        state.right_active = false;
        self.dual = state;
        self.hits.clear();
        self.visible.clear();
        self.context_menu = None;
        self.operation_message =
            Some("Panel cerrado; la cola compartida conserva sus destinos".into());
    }
}

#[cfg(test)]
mod tests;
