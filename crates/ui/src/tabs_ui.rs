//! Per-tab browser snapshots; operations and all supervisors belong to Model.
use crate::{
    model::{Action, Hit, Model},
    painter::{Painter, Rect},
    refresh_selection::Snapshot,
    tabs::{self, Command, Saved, Tab},
};
impl Model {
    fn tab_charge(&self) -> Result<usize, &'static str> {
        files_core::navigation_history::validate(&self.path)?;
        if self.query.len() > 4096 {
            return Err("Filtro demasiado largo para guardar pestaña");
        }
        let selection = if let Some(snapshot) = &self.refresh_selection {
            snapshot.retained_bytes()
        } else {
            Snapshot::estimate(
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
                tabs::SELECTION_LIMIT,
            )?
        };
        if selection > tabs::SELECTION_LIMIT {
            return Err("Selección de pestaña supera256KiB; reduce selección antes de cambiar");
        }
        let charge = selection
            + self.history.retained_bytes()
            + self.path.as_os_str().len()
            + self.query.len()
            + 8192;
        if charge > tabs::SNAPSHOT_LIMIT {
            return Err("Estado de pestaña supera1MiB; reduce selección");
        }
        Ok(charge)
    }
    fn capture_tab(&mut self, charge: usize) -> Result<Saved, &'static str> {
        let selection = if let Some(snapshot) = self.refresh_selection.take() {
            snapshot
        } else {
            Snapshot::capture_with_anchor(
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
                tabs::SELECTION_LIMIT,
            )?
        };
        Ok(Saved {
            path: self.path.clone(),
            history: std::mem::take(&mut self.history),
            selection,
            view: self.view,
            sort: self.sort,
            descending: self.descending,
            query: self.query.clone(),
            scroll: self.scroll,
            grid_zoom: self.grid_zoom,
            pan_x: self.pan_x,
            charge,
        })
    }
    pub(crate) fn restore_tab(&mut self, saved: Saved) {
        self.view = saved.view;
        self.load_location(saved.path);
        self.folder_restore = None;
        self.history = saved.history;
        self.query = saved.query;
        self.sort = saved.sort;
        self.descending = saved.descending;
        self.grid_zoom = saved.grid_zoom;
        self.pan_x = saved.pan_x;
        self.scroll = saved.scroll;
        self.refresh_selection = Some(saved.selection);
        self.settings = false;
        self.help = None;
        self.text_viewport = Default::default();
        self.content_height = 0;
    }
    /// Add metadata only; the active listing/search and worker generation stay untouched.
    #[cfg(test)]
    pub(crate) fn background_tab(
        &mut self,
        path: &std::path::Path,
        focus: Option<&std::ffi::OsStr>,
    ) -> Result<u64, &'static str> {
        self.background_tabs(std::iter::once((path, focus)))
            .map(|ids| ids[0])
    }
    pub(crate) fn background_tabs<'a>(
        &mut self,
        targets: impl Iterator<Item = (&'a std::path::Path, Option<&'a std::ffi::OsStr>)> + Clone,
    ) -> Result<Vec<u64>, &'static str> {
        if self.closing
            || self.mutations.busy
            || self.mutation_applied
            || self.rename_dialog.is_some()
            || self.properties_dialog.is_some()
            || self.copy_form.is_some()
            || self.notice_open
            || self.editing
        {
            return Err("Termina la edición u operación antes de abrir otra pestaña");
        }
        let count = targets.clone().count();
        if count == 0 {
            return Err("No quedan resultados marcados; vuelve a elegir");
        }
        if count > tabs::MAX_TABS.saturating_sub(self.tabs.rows.len()) {
            return Err(
                "Máximo ocho pestañas; el lote no cabe, reduce las marcas o cierra pestañas",
            );
        }
        let history = files_core::navigation_history::History::default();
        let mut total = 0usize;
        for (path, focus) in targets.clone() {
            files_core::navigation_history::validate(path)?;
            if focus.is_some_and(|n| {
                n.is_empty()
                    || n == "."
                    || n == ".."
                    || n.as_encoded_bytes().iter().any(|b| matches!(b, 0 | b'/'))
            }) {
                return Err("Nombre enfocado inválido para la pestaña");
            }
            let charge =
                Snapshot::estimate(focus.into_iter(), focus, focus, tabs::SELECTION_LIMIT)?
                    + history.retained_bytes()
                    + path.as_os_str().len()
                    + 8192;
            total = total
                .checked_add(charge)
                .ok_or("Presupuesto de pestañas agotado")?;
        }
        self.tabs
            .admit_with_reserved(total, 0, self.dual.other_stored())?;
        // Construct every bounded snapshot before publishing any new tab/ID.
        let prepared = targets
            .map(|(path, focus)| {
                let selection = Snapshot::capture_with_anchor(
                    focus.into_iter(),
                    focus,
                    focus,
                    tabs::SELECTION_LIMIT,
                )?;
                let charge = selection.retained_bytes()
                    + history.retained_bytes()
                    + path.as_os_str().len()
                    + 8192;
                Ok(Saved {
                    path: path.to_path_buf(),
                    history: Default::default(),
                    selection,
                    view: self.view,
                    sort: self.sort,
                    descending: self.descending,
                    query: String::new(),
                    scroll: 0,
                    grid_zoom: self.grid_zoom,
                    pan_x: 0.,
                    charge,
                })
            })
            .collect::<Result<Vec<_>, &'static str>>()?;
        let mut ids = Vec::with_capacity(count);
        for saved in prepared {
            let id = self.tabs.fresh_id();
            let path = saved.path.clone();
            self.tabs.rows.push(Tab {
                id,
                path,
                saved: Some(saved),
            });
            ids.push(id);
        }
        Ok(ids)
    }
    pub fn tab_command(&mut self, command: Command) {
        if self.closing {
            return;
        }
        // Apply/undo/redo and edits cannot race a tab change. Their worker still
        // refreshes its captured directory under this explicit UI invariant.
        if self.mutations.busy
            || self.mutation_applied
            || self.rename_dialog.is_some()
            || self.properties_dialog.is_some()
            || self.copy_form.is_some()
            || self.notice_open
            || self.editing
        {
            self.error = "Termina o cancela la edición u operación antes de cambiar pestaña".into();
            return;
        }
        let command = if let Command::Cycle(delta) = command {
            let i = self.tabs.index(self.tabs.active).expect("active tab");
            let target = (i as i32 + delta).rem_euclid(self.tabs.rows.len() as i32) as usize;
            Command::Select(self.tabs.rows[target].id)
        } else {
            command
        };
        if let Command::Select(id) = command
            && (id == self.tabs.active || self.tabs.index(id).is_none())
        {
            return;
        }
        if let Command::Close(id) = command {
            let Some(index) = self.tabs.index(id) else {
                return;
            };
            if id != self.tabs.active {
                let tab = self.tabs.rows.remove(index);
                self.tabs.push_closed(tab.saved.expect("inactive saved"));
                self.hits.clear();
                return;
            }
            if self.tabs.rows.len() == 1 {
                if self.dual.enabled {
                    self.close_focused_pane();
                    return;
                }
                self.closing = true;
                return;
            }
        }
        if matches!(command, Command::New | Command::Reopen)
            && self.tabs.rows.len() == tabs::MAX_TABS
        {
            self.error = "Máximo ocho pestañas; cierra una antes de abrir otra".into();
            return;
        }
        if matches!(command, Command::Reopen) && self.tabs.closed.is_empty() {
            return;
        }
        let incoming_id = match command {
            Command::Select(id) => Some(id),
            Command::Close(_) => {
                let i = self.tabs.index(self.tabs.active).unwrap();
                Some(
                    self.tabs.rows[if i + 1 < self.tabs.rows.len() {
                        i + 1
                    } else {
                        i - 1
                    }]
                    .id,
                )
            }
            _ => None,
        };
        let released = incoming_id
            .and_then(|id| self.tabs.rows[self.tabs.index(id)?].saved.as_ref())
            .map_or(0, |s| s.charge)
            + if matches!(command, Command::Reopen) {
                self.tabs.closed.last().map_or(0, |s| s.charge)
            } else if matches!(command, Command::Close(_))
                && self.tabs.closed.len() == tabs::MAX_TABS
            {
                self.tabs.closed[0].charge
            } else {
                0
            };
        let charge = match self.tab_charge().and_then(|n| {
            self.tabs
                .admit_with_reserved(n, released, self.dual.other_stored())
                .map(|_| n)
        }) {
            Ok(n) => n,
            Err(e) => {
                self.error = format!("Pestañas: {e}");
                return;
            }
        };
        if !self.save_layout() {
            return;
        }
        let outgoing = match self.capture_tab(charge) {
            Ok(s) => s,
            Err(e) => {
                self.error = e.into();
                return;
            }
        };
        let old = self.tabs.index(self.tabs.active).unwrap();
        let incoming = match command {
            Command::New => {
                let id = self.tabs.fresh_id();
                let path = outgoing.path.clone();
                let saved = Saved {
                    path: path.clone(),
                    history: Default::default(),
                    selection: Snapshot::capture(std::iter::empty(), None).expect("empty snapshot"),
                    view: outgoing.view,
                    sort: outgoing.sort,
                    descending: outgoing.descending,
                    query: String::new(),
                    scroll: 0,
                    grid_zoom: outgoing.grid_zoom,
                    pan_x: 0.,
                    charge: 0,
                };
                self.tabs.rows[old].saved = Some(outgoing);
                self.tabs.rows.push(Tab {
                    id,
                    path,
                    saved: None,
                });
                self.tabs.active = id;
                saved
            }
            Command::Reopen => {
                let saved = self.tabs.closed.pop().unwrap();
                let id = self.tabs.fresh_id();
                self.tabs.rows[old].saved = Some(outgoing);
                self.tabs.rows.push(Tab {
                    id,
                    path: saved.path.clone(),
                    saved: None,
                });
                self.tabs.active = id;
                saved
            }
            Command::Select(id) | Command::Close(id) => {
                let next = if matches!(command, Command::Close(_)) {
                    incoming_id.unwrap()
                } else {
                    id
                };
                let index = self.tabs.index(next).unwrap();
                let saved = self.tabs.rows[index].saved.take().unwrap();
                if matches!(command, Command::Close(_)) {
                    self.tabs.rows.remove(old);
                    self.tabs.push_closed(outgoing);
                } else {
                    self.tabs.rows[old].saved = Some(outgoing);
                }
                self.tabs.active = next;
                saved
            }
            Command::Cycle(_) => unreachable!(),
        };
        self.restore_tab(incoming);
    }
}
/// Horizontal strip never enters the draggable header. Stable IDs, clipped label
/// targets and independent24px close targets; reveal active tab at narrow widths.
pub fn paint(p: &mut dyn Painter, m: &mut Model, w: i32) {
    let c = m.palette.clone();
    let y = 58;
    p.rect(Rect::new(0, y, w, tabs::HEIGHT), c.darker_background);
    p.rect(Rect::new(0, y + tabs::HEIGHT - 1, w, 1), c.muted);
    let slots = ((w - 106).max(120) / 180).max(1) as usize;
    let active = m.tabs.index(m.tabs.active).unwrap_or(0);
    if active < m.tabs.first {
        m.tabs.first = active;
    }
    if active >= m.tabs.first + slots {
        m.tabs.first = active + 1 - slots;
    }
    m.tabs.first = m.tabs.first.min(m.tabs.rows.len().saturating_sub(1));
    let mut controls = Vec::with_capacity(20);
    let mut x = 8;
    for tab in m.tabs.rows.iter().skip(m.tabs.first).take(slots) {
        let width = 180.min(w - x - 100).max(64);
        let area = Rect::new(x, y + 3, width, 26);
        let on = tab.id == m.tabs.active;
        p.rounded_rect(
            area,
            5 * m.radius,
            if on {
                c.lighter_background
            } else {
                c.darker_background
            },
        );
        if on {
            p.rect(
                Rect::new(x + 4, y + tabs::HEIGHT - 3, width - 8, 2),
                c.accent,
            );
        }
        let label = tab
            .path
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("/"));
        let text = files_core::name_label::label(label);
        p.clip(Rect::new(x + 8, y + 3, width - 38, 26));
        p.text(
            Rect::new(x + 8, y + 3, width - 38, 26),
            &text,
            12,
            if on { c.foreground } else { c.dark_foreground },
            true,
            false,
        );
        p.unclip();
        controls.push((Rect::new(x, y + 3, width - 28, 26), Command::Select(tab.id)));
        p.text(
            Rect::new(x + width - 27, y + 4, 24, 24),
            crate::fonts::icon("󰅖", "×"),
            13,
            c.dark_foreground,
            true,
            false,
        );
        controls.push((
            Rect::new(x + width - 27, y + 4, 24, 24),
            Command::Close(tab.id),
        ));
        x += width + 4;
    }
    let new = Rect::new(w - 91, y + 4, 28, 24);
    p.text(new, crate::fonts::icon("", "+"), 13, c.accent, true, false);
    controls.push((new, Command::New));
    for (x, delta, label) in [
        (w - 61, -1, crate::fonts::icon("", "‹")),
        (w - 33, 1, crate::fonts::icon("", "›")),
    ] {
        let r = Rect::new(x, y + 4, 26, 24);
        p.text(r, label, 14, c.foreground, true, false);
        controls.push((r, Command::Cycle(delta)));
    }
    for (rect, command) in controls {
        m.hits.push(Hit {
            generation: m.generation,
            rect,
            action: Action::Tab(command),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        time::{Duration, Instant},
    };
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = files_core::theme::home()
                .join(".cache/files-test")
                .join(format!(
                    "tabs-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos()
                ));
            fs::create_dir_all(root.join("A")).unwrap();
            fs::create_dir(root.join("B")).unwrap();
            for n in ["A", "B"] {
                fs::write(root.join(n).join("same.txt"), n).unwrap();
                fs::write(root.join(n).join("other.txt"), n).unwrap();
                fs::write(root.join(n).join(".hidden"), n).unwrap();
            }
            Self(root)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn wait(m: &mut Model) {
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            m.pump();
            if m.loaded && m.refresh_selection.is_none() {
                break;
            }
            assert!(Instant::now() < until, "{}", m.error);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    fn select(m: &mut Model, name: &str, toggle: bool) {
        let i = m.entries.iter().position(|e| e.name == name).unwrap();
        m.select_entry(i, toggle, false);
    }
    #[test]
    fn switches_before_first_batch_keep_names_query_sort_anchor_and_global_hidden() {
        let f = Fixture::new();
        let mut m = Model::new(f.0.join("A"));
        wait(&mut m);
        m.show_hidden = true;
        m.rebuild();
        select(&mut m, "other.txt", false);
        select(&mut m, "same.txt", true);
        m.view = crate::model::View::Grid;
        m.grid_zoom = 1.4;
        m.pan_x = 57.;
        m.scroll = 321;
        m.sort = files_core::listing::Sort::Size;
        m.descending = true;
        let a = m.tabs.active;
        m.tab_command(Command::New);
        let b = m.tabs.active;
        m.navigate(f.0.join("B"));
        // B's path may have no batches yet; its old indices must never select A.
        m.tab_command(Command::Select(a));
        m.tab_command(Command::Select(b));
        m.tab_command(Command::Select(a));
        wait(&mut m);
        assert_eq!(m.path, f.0.join("A"));
        assert!(
            m.view == crate::model::View::Grid
                && m.grid_zoom == 1.4
                && m.pan_x == 57.
                && m.scroll == 321
                && m.descending
        );
        assert!(matches!(m.sort, files_core::listing::Sort::Size));
        assert_eq!(m.selection.len(), 2);
        assert_eq!(m.entries[m.anchor.unwrap()].name, "other.txt");
        assert_eq!(m.entries[m.selected.unwrap()].name, "same.txt");
        m.tab_command(Command::Select(b));
        wait(&mut m);
        assert_eq!(m.path, f.0.join("B"));
        m.query = "same".into();
        m.rebuild();
        select(&mut m, "same.txt", false);
        m.tab_command(Command::Select(a));
        m.show_hidden = false;
        m.rebuild();
        wait(&mut m);
        m.tab_command(Command::Select(b));
        wait(&mut m);
        assert_eq!(m.query, "same");
        assert!(!m.show_hidden);
        assert_eq!(m.selection.len(), 1);
        assert_eq!(m.entries[m.selected.unwrap()].name, "same.txt");
        assert!(m.history.can_back());
        assert_eq!(fs::read(m.path.join("same.txt")).unwrap(), b"B");
    }
    #[test]
    fn refresh_selection_intersects_changed_filter_and_hidden_before_copy_rename() {
        let f = Fixture::new();
        let mut m = Model::new(f.0.join("A"));
        wait(&mut m);
        m.show_hidden = true;
        m.rebuild();
        m.action(Action::Selection(crate::selection_tools::Command::All));
        m.refresh_current();
        m.query = "same".into();
        m.show_hidden = false;
        wait(&mut m);
        assert_eq!(m.selection.len(), 1);
        assert_eq!(m.entries[m.selected.unwrap()].name, "same.txt");
        m.open_copy();
        let form = m.copy_form.as_ref().unwrap();
        assert_eq!(form.names.as_ref(), [std::ffi::OsString::from("same.txt")]);
        m.copy_form = None;
        m.open_rename(crate::rename_dialog::Mode::Rename);
        assert_eq!(
            m.rename_dialog.as_ref().unwrap().selected.as_ref(),
            [std::ffi::OsString::from("same.txt")]
        );
    }
    #[test]
    fn stable_ids_close_reopen_limit_and_last_tab_close_request() {
        let f = Fixture::new();
        let mut m = Model::new(f.0.join("A"));
        wait(&mut m);
        let a = m.tabs.active;
        m.tab_command(Command::New);
        let b = m.tabs.active;
        m.navigate(f.0.join("B"));
        m.tab_command(Command::Close(b));
        assert_eq!(m.tabs.active, a);
        m.tab_command(Command::Select(b));
        assert_eq!(m.tabs.active, a);
        m.tab_command(Command::Reopen);
        assert!(m.tabs.active > b);
        wait(&mut m);
        assert_eq!(m.path, f.0.join("B"));
        for _ in 0..6 {
            m.tab_command(Command::New);
        }
        assert_eq!(m.tabs.rows.len(), 8);
        let active = m.tabs.active;
        m.tab_command(Command::New);
        assert_eq!(m.tabs.active, active);
        assert!(m.error.contains("ocho"));
        let ids = m
            .tabs
            .rows
            .iter()
            .map(|t| t.id)
            .filter(|id| *id != active)
            .collect::<Vec<_>>();
        for id in ids {
            m.tab_command(Command::Close(id));
        }
        assert_eq!(m.tabs.rows.len(), 1);
        m.tab_command(Command::Close(active));
        assert!(m.closing);
    }
    #[test]
    fn budget_rejection_and_apply_lock_preserve_active_listing() {
        let f = Fixture::new();
        let mut m = Model::new(f.0.join("A"));
        wait(&mut m);
        let i = m.entries.iter().position(|e| e.name == "same.txt").unwrap();
        m.entries[i].name = std::ffi::OsString::from("x".repeat(4097));
        m.selection.clear();
        m.selection.insert(i);
        m.selected = Some(i);
        let generation = m.generation;
        m.tab_command(Command::New);
        assert_eq!(m.tabs.rows.len(), 1);
        assert_eq!(m.generation, generation);
        assert!(m.error.contains("Pestañas"));
        m.entries[i].name = "same.txt".into();
        m.mutation_applied = true;
        m.tab_command(Command::New);
        assert_eq!(m.tabs.rows.len(), 1);
        m.mutation_applied = false;
        m.tab_command(Command::New);
        assert_eq!(m.tabs.rows.len(), 2);
    }
    #[test]
    fn snapshot_selection_is_pruned_when_hidden_changes_while_inactive() {
        let f = Fixture::new();
        let mut m = Model::new(f.0.join("A"));
        wait(&mut m);
        m.show_hidden = true;
        m.rebuild();
        select(&mut m, ".hidden", false);
        let a = m.tabs.active;
        m.tab_command(Command::New);
        m.show_hidden = false;
        m.tab_command(Command::Select(a));
        wait(&mut m);
        m.rebuild();
        m.pump();
        m.rebuild();
        assert!(m.selected.is_none() && m.selection.is_empty() && m.anchor.is_none());
        m.open_copy();
        assert!(m.copy_form.is_none());
        m.notice_open = false;
        m.open_rename(crate::rename_dialog::Mode::Rename);
        assert!(m.rename_dialog.is_none());
        crate::input(&mut m, "same");
        assert_eq!(m.selection.len(), 1);
        assert_eq!(m.entries[m.selected.unwrap()].name, "same.txt");
        assert_eq!(m.path.as_path(), f.0.join("A").as_path());
    }
    #[test]
    fn undo_a_after_failed_create_b_keeps_b_selection_and_refresh_generation() {
        let f = Fixture::new();
        let a = f.0.join("A");
        let b = f.0.join("B");
        let mut m = Model::new(a.clone());
        wait(&mut m);
        let aid = m.tabs.active;
        let prepared = files_core::rename_transaction::PreparedRename::capture(
            &a,
            vec![("same.txt".into(), "renamed.txt".into())],
        )
        .unwrap();
        assert!(
            m.mutations
                .submit(crate::rename_job::Request::Apply { id: 1, prepared })
                .is_ok()
        );
        m.mutation_id = 1;
        m.mutation_applied = true;
        let deadline = Instant::now() + Duration::from_secs(10);
        let drain = |m: &mut Model| {
            while !m.mutations_drained() {
                m.pump();
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
        };
        drain(&mut m);
        assert!(m.can_undo);
        m.tab_command(Command::New);
        m.navigate(b.clone());
        wait(&mut m);
        m.open_rename(crate::rename_dialog::Mode::File);
        m.rename_dialog.as_mut().unwrap().fields[0] = "same.txt".into();
        m.apply_rename();
        drain(&mut m);
        m.notice_open = false;
        wait(&mut m);
        assert!(m.can_undo);
        m.query = "same".into();
        m.rebuild();
        select(&mut m, "same.txt", false);
        let generation = m.generation;
        m.undo_rename(false);
        drain(&mut m);
        assert_eq!(m.generation, generation);
        assert_eq!(m.path, b);
        assert_eq!(m.query, "same");
        assert_eq!(m.entries[m.selected.unwrap()].name, "same.txt");
        assert_eq!(fs::read(a.join("same.txt")).unwrap(), b"A");
        assert_eq!(fs::read(b.join("same.txt")).unwrap(), b"B");
        assert!(!a.join("renamed.txt").exists());
        m.tab_command(Command::Select(aid));
        wait(&mut m);
        assert!(m.entries.iter().any(|e| e.name == "same.txt"));
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use std::{
        fs,
        time::{Duration, Instant},
    };
    #[test]
    fn copy_b_finishes_while_a_active_without_refreshing_a_then_b_reloads_and_last_close_drains() {
        let base = files_core::theme::home()
            .join(".cache/files-test")
            .join(format!(
                "tab-copy-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        let a = base.join("A");
        let b = base.join("B");
        let src = base.join("source");
        for p in [&a, &b, &src] {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(a.join("same.txt"), b"A").unwrap();
        fs::write(b.join("same.txt"), b"B").unwrap();
        fs::write(src.join("copied.txt"), b"C").unwrap();
        let mut m = Model::new(a.clone());
        let aid = m.tabs.active;
        m.tab_command(Command::New);
        let bid = m.tabs.active;
        m.navigate(b.clone());
        let paths = vec![files_core::copy_engine::CopyRequest {
            source: src.join("copied.txt"),
            destination: b.join("copied.txt"),
        }];
        m.copy
            .enqueue(crate::copy_job::Request::new(1, paths, Default::default()).start_paused())
            .unwrap();
        m.copy_locations.insert(1, b.clone());
        m.tab_command(Command::Select(aid));
        let generation = m.generation;
        let deadline = Instant::now() + Duration::from_secs(10);
        while m.copy.active_id().is_none() {
            m.pump();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        m.copy.resume(1);
        while m.copy_busy() {
            m.pump();
            assert!(Instant::now() < deadline, "{}", m.error);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(m.path, a);
        assert_eq!(m.generation, generation, "copy to B must not refresh A");
        assert_eq!(fs::read(a.join("same.txt")).unwrap(), b"A");
        m.tab_command(Command::Select(bid));
        while !m.loaded {
            m.pump();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(m.entries.iter().any(|e| e.name == "copied.txt"));
        assert_eq!(fs::read(b.join("same.txt")).unwrap(), b"B");
        m.tab_command(Command::Close(aid));
        let paths = vec![files_core::copy_engine::CopyRequest {
            source: src.join("copied.txt"),
            destination: b.join("second.txt"),
        }];
        m.copy
            .enqueue(crate::copy_job::Request::new(2, paths, Default::default()).start_paused())
            .unwrap();
        m.pump();
        m.tab_command(Command::Close(bid));
        assert!(m.closing);
        assert!(!m.close_ready());
        while !m.close_ready() {
            m.pump();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(m.copy.is_closed());
        assert!(!b.join("second.txt").exists());
        drop(m);
        fs::remove_dir_all(base).unwrap();
    }
}
