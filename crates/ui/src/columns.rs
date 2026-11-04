//! Miller chain: one active Model listing plus one reusable lookahead worker.
//! Ancestors own only bounded browser data, never a Model or media supervisor.
use crate::{
    model::{Model, View},
    worker::{Update, Worker},
};
use files_core::{
    listing::{Entry, Sort},
    natural_sort::NaturalKeyRef,
};
use std::{collections::HashSet, path::PathBuf};
pub const MAX_COLUMNS: usize = 8;
pub const MAX_ENTRIES: usize = 100_000;
pub const DATA_LIMIT: usize = 16 * 1024 * 1024;
const NAME_LIMIT: usize = 8 * 1024 * 1024;
#[derive(Default)]
pub struct Data {
    pub entries: Vec<Entry>,
    pub order: Vec<usize>,
    pub selected: Option<usize>,
    pub selection: HashSet<usize>,
    pub anchor: Option<usize>,
    pub query: String,
    pub scroll: i32,
    pub loaded: bool,
    pub error: String,
    pub descending: bool,
    pub requested: HashSet<usize>,
    pending_selection: Option<crate::refresh_selection::Snapshot>,
    sort: Option<Sort>,
    viewport: Option<(std::ffi::OsString, i32)>,
}
impl Data {
    pub fn charge(&self) -> usize {
        charge(&self.entries, &self.order, &self.selection)
            + self.query.capacity()
            + self.error.capacity()
            + self.requested.capacity() * 24
            + self
                .pending_selection
                .as_ref()
                .map_or(0, |s| s.retained_bytes())
            + self.viewport.as_ref().map_or(0, |(n, _)| n.capacity() + 32)
            + 512
    }
    fn select(&mut self, i: usize, toggle: bool, extend: bool) {
        if !self.order.contains(&i) {
            return;
        }
        if extend {
            let from = self
                .anchor
                .or(self.selected)
                .and_then(|a| self.order.iter().position(|j| *j == a));
            let to = self.order.iter().position(|j| *j == i);
            if let (Some(a), Some(b)) = (from, to) {
                if !toggle {
                    self.selection.clear();
                }
                self.selection
                    .extend(self.order[a.min(b)..=a.max(b)].iter().copied());
            }
        } else if toggle {
            if !self.selection.insert(i) {
                self.selection.remove(&i);
            }
        } else {
            self.selection.clear();
            self.selection.insert(i);
            self.anchor = Some(i);
        }
        self.selected = Some(i);
        if self.anchor.is_none() {
            self.anchor = Some(i);
        }
    }
    fn restore(&mut self) {
        if !self.loaded {
            return;
        }
        if let Some(snapshot) = self.pending_selection.take() {
            self.selection.clear();
            self.selected = None;
            self.anchor = None;
            for &i in &self.order {
                let name = &self.entries[i].name;
                if snapshot.contains(name) {
                    self.selection.insert(i);
                }
                if snapshot.focused(name) {
                    self.selected = Some(i);
                }
                if snapshot.anchored(name) {
                    self.anchor = Some(i);
                }
            }
        }
        if let Some((name, offset)) = self.viewport.take()
            && let Some(rank) = self
                .order
                .iter()
                .position(|&i| self.entries[i].name == name)
        {
            self.scroll = (rank as i32 * 36).saturating_sub(offset).max(0);
        }
    }
    fn rebuild(&mut self, hidden: bool) {
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
        if self.descending {
            self.order.reverse();
        }
        self.order.sort_by_key(|&i| !self.entries[i].is_dir());
        let entries = &self.entries;
        let query = &self.query;
        self.selection
            .retain(|&i| entries.get(i).is_some_and(|e| accepts(e, hidden, query)));
        self.selected = self
            .selected
            .filter(|&i| entries.get(i).is_some_and(|e| accepts(e, hidden, query)));
        self.anchor = self
            .anchor
            .filter(|&i| entries.get(i).is_some_and(|e| accepts(e, hidden, query)));
    }
}
pub fn accepts(e: &Entry, hidden: bool, query: &str) -> bool {
    files_core::visibility::accepts(&e.name, hidden)
        && (query.is_empty()
            || files_core::name_filter::Filter::new(query)
                .score(e)
                .is_some())
}
pub fn charge(entries: &Vec<Entry>, order: &Vec<usize>, selected: &HashSet<usize>) -> usize {
    entries.capacity() * std::mem::size_of::<Entry>()
        + entries.iter().map(|e| e.name.capacity()).sum::<usize>()
        + order.capacity() * std::mem::size_of::<usize>()
        + selected.capacity() * 24
}
pub struct Column {
    pub id: u64,
    pub watch_id: u64,
    pub path: PathBuf,
    pub data: Data,
}
pub struct DeferredMenu {
    pub id: u64,
    pub generation: u64,
    pub name: std::ffi::OsString,
    pub rect: crate::painter::Rect,
}
#[derive(Default)]
pub struct State {
    pub columns: Vec<Column>,
    pub active: usize,
    pub horizontal: i32,
    pub reveal: Option<usize>,
    pub worker: Option<Worker>,
    pub(super) pending: Option<u64>,
    next: u64,
    generation: u64,
    tab: u64,
    hidden: bool,
    enabled: bool,
    pub notice: Option<&'static str>,
    pub area: crate::painter::Rect,
    pub deferred_menu: Option<DeferredMenu>,
}
impl State {
    fn id(&mut self) -> u64 {
        self.next = self.next.checked_add(1).expect("column ID exhausted");
        self.next
    }
    pub fn clear(&mut self) {
        self.columns.clear();
        self.active = 0;
        self.horizontal = 0;
        self.pending = None;
        if self.enabled
            && let Some(w) = &mut self.worker
        {
            w.suspend();
        }
        self.enabled = false;
        self.deferred_menu = None;
        self.notice = None;
        self.reveal = None;
    }
    fn root(&mut self, m: &Model) {
        self.clear();
        self.enabled = true;
        self.generation = m.generation;
        self.hidden = m.show_hidden;
        self.tab = m.tabs.active;
        let id = self.id();
        self.columns.push(Column {
            id,
            watch_id: id,
            path: m.path.clone(),
            data: Data::default(),
        });
    }
    pub fn retained(&self) -> usize {
        self.columns
            .iter()
            .map(|c| c.data.charge() + c.path.capacity() + 128)
            .sum()
    }
    pub fn accept_batch(&self, m: &Model, batch: &[Entry], foreground: bool) -> bool {
        let entries = if foreground {
            &m.entries
        } else {
            match self
                .pending
                .and_then(|id| self.columns.iter().find(|c| c.id == id))
            {
                Some(c) => &c.data.entries,
                None => return false,
            }
        };
        if entries.len().saturating_add(batch.len()) > MAX_ENTRIES {
            return false;
        }
        let names = entries
            .iter()
            .chain(batch)
            .try_fold(0usize, |n, e| n.checked_add(e.name.len()));
        if names.is_none_or(|n| n > NAME_LIMIT) {
            return false;
        }
        // Reserve entry growth plus order, selection and sort keys before extend.
        // Active existing storage and all cached capacities are counted together.
        let added_entries = entries.len() + batch.len();
        let entry_growth =
            added_entries.saturating_sub(entries.capacity()) * std::mem::size_of::<Entry>();
        let order_capacity = if foreground {
            m.order.capacity()
        } else {
            self.pending
                .and_then(|id| self.columns.iter().find(|c| c.id == id))
                .map_or(0, |c| c.data.order.capacity())
        };
        let order_growth =
            added_entries.saturating_sub(order_capacity) * std::mem::size_of::<usize>();
        let potential_selections = (self
            .columns
            .iter()
            .map(|c| c.data.entries.len())
            .sum::<usize>()
            + m.entries.len()
            + batch.len())
            * 32;
        let projected = self
            .retained()
            .saturating_add(charge(&m.entries, &m.order, &m.selection))
            .saturating_add(m.query.capacity())
            .saturating_add(
                m.refresh_selection
                    .as_ref()
                    .map_or(0, |s| s.retained_bytes()),
            )
            .saturating_add(
                m.refresh_viewport
                    .as_ref()
                    .map_or(0, |v| v.retained_bytes()),
            )
            .saturating_add(entry_growth)
            .saturating_add(order_growth)
            .saturating_add(batch.iter().map(|e| e.name.capacity()).sum::<usize>())
            .saturating_add(potential_selections);
        projected <= DATA_LIMIT
    }
    fn truncate(&mut self, at: usize) {
        self.columns.truncate(at);
        if !self
            .pending
            .is_some_and(|id| self.columns.iter().any(|c| c.id == id))
        {
            self.pending = None;
            if let Some(w) = &mut self.worker {
                w.suspend();
            }
        }
    }
    pub fn refresh_cached(&mut self, m: &Model, watch_id: u64, path: &std::path::Path) -> bool {
        let Some(index) = self
            .columns
            .iter()
            .position(|c| c.watch_id == watch_id && c.path == path)
        else {
            return false;
        };
        if index == self.active
            || !self.columns[index].data.loaded
            || self
                .pending
                .and_then(|id| self.columns.iter().find(|c| c.id == id))
                .is_some_and(|c| !c.data.loaded)
        {
            return false;
        }
        let d = &self.columns[index].data;
        let used = self
            .retained()
            .saturating_add(charge(&m.entries, &m.order, &m.selection))
            .saturating_add(m.query.capacity())
            .saturating_add(
                m.refresh_selection
                    .as_ref()
                    .map_or(0, |s| s.retained_bytes()),
            )
            .saturating_add(
                m.refresh_viewport
                    .as_ref()
                    .map_or(0, |v| v.retained_bytes()),
            );
        // Existing storage remains live while the snapshot is admitted.
        let limit = DATA_LIMIT
            .saturating_sub(used)
            .min(crate::tabs::SELECTION_LIMIT);
        let snapshot = match crate::refresh_selection::Snapshot::capture_with_anchor(
            d.entries
                .iter()
                .enumerate()
                .filter(|(i, _)| d.selection.contains(i))
                .map(|(_, e)| e.name.as_os_str()),
            d.selected
                .and_then(|i| d.entries.get(i))
                .map(|e| e.name.as_os_str()),
            d.anchor
                .and_then(|i| d.entries.get(i))
                .map(|e| e.name.as_os_str()),
            limit.saturating_sub(4160),
        ) {
            Ok(s) => s,
            Err(_) => {
                self.notice =
                    Some("Actualización pendiente · presupuesto de columnas insuficiente");
                return false;
            }
        };
        let viewport = (d.scroll > 0)
            .then(|| {
                let rank = (d.scroll / 36) as usize;
                let e = d.entries.get(*d.order.get(rank)?)?;
                (e.name.len() <= 4096).then(|| (e.name.clone(), rank as i32 * 36 - d.scroll))
            })
            .flatten();
        let id = self.id();
        let c = &mut self.columns[index];
        c.id = id;
        let query = std::mem::take(&mut c.data.query);
        let descending = c.data.descending;
        let sort = c.data.sort;
        let scroll = c.data.scroll;
        c.data = Data {
            query,
            descending,
            sort,
            scroll,
            pending_selection: Some(snapshot),
            viewport,
            ..Data::default()
        };
        if let Some(w) = &mut self.worker {
            w.restart(path.to_path_buf());
        } else {
            self.worker = Some(Worker::start(path.to_path_buf()));
        }
        self.pending = Some(id);
        true
    }
    pub fn pump(&mut self, m: &mut Model) -> bool {
        if m.view != View::Columns || m.closing {
            let changed = self.enabled;
            self.clear();
            return changed;
        }
        let mut dirty = false;
        if self
            .columns
            .get(self.active)
            .is_none_or(|c| c.path != m.path)
            || self.tab != m.tabs.active
        {
            self.root(m);
            dirty = true;
        }
        if self.generation != m.generation {
            self.truncate(self.active + 1);
            let id = self.id();
            self.columns[self.active].id = id;
            self.generation = m.generation;
            dirty = true;
        }
        if self.hidden != m.show_hidden {
            self.hidden = m.show_hidden;
            for c in &mut self.columns {
                c.data.rebuild(m.show_hidden);
            }
            dirty = true;
        }
        for _ in 0..16 {
            let update = self.worker.as_ref().and_then(|w| w.rx.try_recv().ok());
            let Some(update) = update else { break };
            let Some(index) = self
                .pending
                .and_then(|id| self.columns.iter().position(|c| c.id == id))
            else {
                continue;
            };
            dirty = true;
            match update {
                Update::Entries(batch) => {
                    if !self.accept_batch(m, &batch, false) {
                        self.columns[index].data.error="Listado parcial: presupuesto de columnas agotado; abre esta carpeta para continuar".into();
                        self.columns[index].data.loaded = true;
                        self.pending = None;
                        if let Some(w) = &mut self.worker {
                            w.suspend();
                        }
                    } else {
                        self.columns[index].data.entries.reserve_exact(batch.len());
                        self.columns[index].data.entries.extend(batch);
                    }
                }
                Update::Done => self.columns[index].data.loaded = true,
                Update::Error(e) => {
                    self.columns[index].data.error = e;
                    self.columns[index].data.loaded = true;
                }
                Update::Waiting(true) => {
                    self.columns[index].data.error = crate::worker::CAPACITY_MESSAGE.into()
                }
                Update::Waiting(false) => {
                    if self.columns[index].data.error == crate::worker::CAPACITY_MESSAGE {
                        self.columns[index].data.error.clear();
                    }
                }
                Update::Stats(stats) => {
                    for (i, md) in stats {
                        if let Some(e) = self.columns[index].data.entries.get_mut(i) {
                            e.metadata = md;
                            e.stat_attempted = true;
                        }
                    }
                }
                _ => {}
            }
            self.columns[index].data.rebuild(m.show_hidden);
            self.columns[index].data.restore();
        }
        if self.columns.windows(2).enumerate().any(|(i, pair)| {
            i < self.active
                && pair[0].data.loaded
                && pair[0].data.error.is_empty()
                && pair[0].data.pending_selection.is_none()
                && !pair[0]
                    .data
                    .entries
                    .iter()
                    .any(|e| e.is_dir() && pair[0].path.join(&e.name) == pair[1].path)
        }) {
            self.root(m);
            self.notice = Some("La rama cambió fuera de Files · ruta actual conservada");
            dirty = true;
        }
        if let Some(menu) = self.deferred_menu.take() {
            let current = self
                .columns
                .get(self.active)
                .is_some_and(|c| c.id == menu.id)
                && m.generation == menu.generation;
            if current && !m.loaded {
                self.deferred_menu = Some(menu);
            } else if current
                && !m.selection_refreshing()
                && m.selected
                    .and_then(|i| m.entries.get(i))
                    .is_some_and(|e| e.name == menu.name)
            {
                dirty |= crate::context_menu::open_loaded_selection(
                    m,
                    menu.rect.x,
                    menu.rect.y,
                    menu.rect.w,
                    menu.rect.h,
                );
            }
        }
        if m.loaded
            && !m.selection_refreshing()
            && !self
                .pending
                .and_then(|id| self.columns.iter().find(|c| c.id == id))
                .is_some_and(|c| !c.data.loaded)
        {
            let target = m
                .selected
                .filter(|i| m.selection.len() == 1 && m.selection.contains(i))
                .and_then(|i| m.entries.get(i))
                .filter(|e| e.is_dir() && accepts(e, m.show_hidden, &m.query));
            let path = target.and_then(|e| {
                m.path
                    .as_os_str()
                    .len()
                    .checked_add(e.name.len() + 1)
                    .filter(|n| *n <= 4096)
                    .map(|_| m.path.join(&e.name))
            });
            if let Some(path) = path {
                if self
                    .columns
                    .get(self.active + 1)
                    .is_none_or(|c| c.path != path)
                {
                    self.truncate(self.active + 1);
                    if self.columns.len() == MAX_COLUMNS {
                        self.columns.remove(0);
                        self.active -= 1;
                        self.horizontal = (self.horizontal - 260).max(0);
                        self.notice =
                            Some("Ancestro anterior liberado · ruta superior permite volver");
                    }
                    let id = self.id();
                    if let Some(w) = &mut self.worker {
                        w.restart(path.clone());
                    } else {
                        self.worker = Some(Worker::start(path.clone()));
                    }
                    self.columns.push(Column {
                        id,
                        watch_id: id,
                        path,
                        data: Data::default(),
                    });
                    self.pending = Some(id);
                    self.reveal = Some(self.active + 1);
                    dirty = true;
                }
            } else if self.columns.len() > self.active + 1 {
                self.truncate(self.active + 1);
                dirty = true;
            }
        }
        dirty
    }
}
impl Model {
    pub fn pump_columns(&mut self) -> bool {
        // Exclusive spare ownership, including session/tab restoration that can
        // enter Columns without Action::View. Never create a replacement worker
        // while the same spare is merely dormant in the other mode.
        if self.view == View::Columns && !self.dual.enabled && self.columns.worker.is_none() {
            self.columns.worker = self.dual.other.as_mut().and_then(|b| b.worker.take());
        }

        let mut state = std::mem::take(&mut self.columns);
        let dirty = state.pump(self);
        self.columns = state;
        dirty
    }
    pub fn column_focus(&mut self, id: u64, entry: Option<usize>, toggle: bool, extend: bool) {
        if self.view != View::Columns
            || self.closing
            || self.editing
            || self.mutations.busy
            || self.mutation_applied
            || self.copy_form.is_some()
            || self.rename_dialog.is_some()
            || self.properties_dialog.is_some()
            || self.notice_open
        {
            return;
        }
        self.columns.deferred_menu = None;
        let Some(index) = self.columns.columns.iter().position(|c| c.id == id) else {
            return;
        };
        if index == self.columns.active {
            if let Some(i) = entry {
                self.select_entry(i, toggle, extend);
            }
            return;
        }
        if self.columns.columns[index].data.pending_selection.is_some() {
            self.error = "Espera a que termine la actualización de esa columna".into();
            return;
        }
        if entry.is_some_and(|i| !self.columns.columns[index].data.order.contains(&i)) {
            return;
        }
        // Preflight before moving active Vecs; no clone of entries or raw names.
        if self
            .columns
            .retained()
            .saturating_add(charge(&self.entries, &self.order, &self.selection))
            .saturating_add(crate::tabs::SELECTION_LIMIT)
            > DATA_LIMIT
        {
            self.error="No se cambió columna: reduce selección o vuelve por la ruta superior (presupuesto16MiB)".into();
            return;
        }
        if !self.columns.columns[index].data.loaded
            && self.columns.columns[index].data.pending_selection.is_none()
        {
            self.error = "Espera a que termine el listado de esa columna".into();
            return;
        }
        if self.columns.pending == Some(id) {
            self.columns.pending = None;
            if let Some(w) = &mut self.columns.worker {
                w.suspend();
            }
        }
        let mut incoming = std::mem::take(&mut self.columns.columns[index].data);
        if let Some(i) = entry {
            incoming.select(i, toggle, extend);
        }
        // Snapshot construction can fail; admit it BEFORE moving the current data.
        let incoming_snapshot = if incoming.pending_selection.is_some() {
            None
        } else {
            match crate::refresh_selection::Snapshot::capture_with_anchor(
                incoming
                    .entries
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| incoming.selection.contains(i))
                    .map(|(_, e)| e.name.as_os_str()),
                incoming
                    .selected
                    .and_then(|i| incoming.entries.get(i))
                    .map(|e| e.name.as_os_str()),
                incoming
                    .anchor
                    .and_then(|i| incoming.entries.get(i))
                    .map(|e| e.name.as_os_str()),
                crate::tabs::SELECTION_LIMIT,
            ) {
                Ok(s) => Some(s),
                Err(e) => {
                    self.columns.columns[index].data = incoming;
                    self.error = e.into();
                    return;
                }
            }
        };
        let outgoing = Data {
            entries: std::mem::take(&mut self.entries),
            order: std::mem::take(&mut self.order),
            selected: self.selected,
            selection: std::mem::take(&mut self.selection),
            anchor: self.anchor,
            query: std::mem::take(&mut self.query),
            scroll: self.scroll,
            loaded: self.loaded,
            error: std::mem::take(&mut self.error),
            descending: self.descending,
            requested: HashSet::new(),
            pending_selection: self.refresh_selection.take(),
            sort: Some(self.sort),
            viewport: None,
        };
        self.columns.columns[self.columns.active].data = outgoing;
        let path = self.columns.columns[index].path.clone();
        let _ = self.history.visit(&self.path, &path);
        self.path = path;
        self.columns.active = index;
        self.entries = incoming.entries;
        self.order = incoming.order;
        self.selected = incoming.selected;
        self.selection = incoming.selection;
        self.anchor = incoming.anchor;
        self.query = incoming.query;
        self.scroll = incoming.scroll;
        self.loaded = incoming.loaded;
        self.error = incoming.error;
        self.descending = incoming.descending;
        self.sort = incoming.sort.unwrap_or(Sort::Name);
        self.refresh_selection = incoming.pending_selection.or(incoming_snapshot);
        self.preserve_empty_selection = self.selection.is_empty();
        self.preview.close();
        self.thumbnails.suspend();
        self.sizes.invalidate();
        self.du_requested = None;
        self.du_target = None;
        if let Some(j) = &self.du_job {
            j.cancel();
        }
        self.reset_space_map();
        self.layout_ready = false;
        self.layout_requested = false;
        self.layout_dirty = false;
        self.layout_path = None;
        self.layout = Default::default();
        self.drag = None;
        self.tabs.update_path(&self.path);
        // A cached ancestor is evidence for rendering, not freshness authority.
        // Reload in the active worker; snapshot restores raw selection at Done.
        self.refresh_current();
        self.columns.generation = self.generation;
        self.columns.reveal = Some(index);
        self.hits.clear();
        self.visible.clear();
        self.hovered = None;
        self.session.restore_allowed = false;
    }
    pub fn column_step(&mut self, right: bool) {
        if self.view != View::Columns {
            return;
        }
        self.pump_columns();
        let index = if right {
            self.columns.active.checked_add(1)
        } else {
            self.columns.active.checked_sub(1)
        };
        if let Some(id) = index
            .and_then(|i| self.columns.columns.get(i))
            .filter(|c| c.data.loaded || c.data.pending_selection.is_some())
            .map(|c| c.id)
        {
            // First item only when entering a new, already loaded child by keyboard.
            let entry = if right {
                let c = &self.columns.columns[self.columns.active + 1];
                c.data.selected.or_else(|| c.data.order.first().copied())
            } else {
                None
            };
            self.column_focus(id, entry, false, false);
        } else if !right && let Some(p) = self.path.parent() {
            self.navigate(p.into());
        }
    }
}
#[cfg(test)]
mod tests;
