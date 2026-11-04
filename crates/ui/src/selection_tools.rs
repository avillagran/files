//! Selection acts only on the active loaded browser; no filesystem or path decoding.
use crate::model::{Model, View};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    All,
    Invert,
    Clear,
    Filtered,
}
pub fn is_map(m: &Model) -> bool {
    m.view == View::SpaceMap && !m.dual.enabled
}
pub fn map_stamp(m: &Model) -> Option<(u64, bool)> {
    is_map(m).then(|| (m.space_map.revision(), m.show_hidden))
}
pub fn map_reason(m: &Model, command: Command) -> Option<&'static str> {
    if !is_map(m) || command == Command::Clear {
        return None;
    }
    if command == Command::Filtered {
        return Some("Espacio no aplica el filtro; usa una vista de listado");
    }
    if m.space_map.path() != Some(m.path.as_path())
        || !matches!(m.space_map.view(), crate::space_map_state::View::Ready(_))
    {
        return Some("Espera el cálculo de Espacio");
    }
    if !m.space_geometry.current(
        m.generation,
        m.space_map.revision(),
        m.entries.len(),
        m.show_hidden,
    ) {
        return Some("Espera la proyección vigente del mapa");
    }
    None
}
pub(super) fn blocked(m: &Model) -> bool {
    m.closing
        || !m.loaded
        || m.selection_refreshing()
        || m.editing
        || m.filter_edit.active
        || m.search.visible
        || m.bookmarks.panel.is_some()
        || m.commands.is_some()
        || m.size_panel.is_some()
        || m.sftp.form.is_some()
        || m.preview.visible()
        || m.help.is_some()
        || m.rename_dialog.is_some()
        || m.copy_form.is_some()
        || m.properties_dialog.is_some()
        || m.mutation_applied
}
impl Model {
    pub fn selection_command(&mut self, command: Command) {
        if blocked(self) {
            return;
        }
        if let Some(reason) = map_reason(self, command) {
            self.error = format!("Selección: {reason}");
            return;
        }
        if command == Command::Filtered {
            if self.query.trim().is_empty() {
                self.error =
                    "Selección: escribe primero un filtro o patrón; por ejemplo *.rs".into();
                return;
            }
            if let Some(error) = files_core::name_filter::Filter::new(&self.query).error() {
                self.error = format!("Selección: {error}");
                return;
            }
        }
        self.session.restore_allowed = false;
        self.text_paste.invalidate();
        self.columns.deferred_menu = None;
        self.drag = None;
        if self.error.starts_with("Selección:") {
            self.error.clear();
        }
        let map = is_map(self);
        let order = if map {
            self.space_geometry.navigation_indices()
        } else {
            self.order.as_slice()
        };
        let focus = self.selected.filter(|i| order.contains(i));
        // Only inversion retains old marks; overwrite/clear need no filter or scan.
        if command == Command::Invert {
            let filter = (!map).then(|| files_core::name_filter::Filter::new(&self.query));
            self.selection.retain(|&i| {
                self.entries.get(i).is_some_and(|e| {
                    if map {
                        self.space_geometry.contains_entry(i)
                    } else {
                        files_core::visibility::accepts(&e.name, self.show_hidden)
                            && filter.as_ref().is_some_and(|f| f.score(e).is_some())
                    }
                })
            });
        }
        match command {
            Command::All | Command::Filtered => {
                self.selection.clear();
                self.selection.extend(order.iter().copied());
            }
            Command::Invert => {
                for &i in order {
                    if !self.selection.remove(&i) {
                        self.selection.insert(i);
                    }
                }
            }
            Command::Clear => self.selection.clear(),
        }
        // An empty operation selection has no fallback focus that F5/F6 could capture.
        self.selected = if self.selection.is_empty() {
            None
        } else {
            focus.or_else(|| order.iter().copied().find(|i| self.selection.contains(i)))
        };
        self.anchor = self.selected;
        self.preserve_empty_selection = self.selection.is_empty();
        if map && self.selected.is_some() {
            self.scroll = self
                .space_geometry
                .reveal_scroll(self.selected, self.scroll);
        }
        if command == Command::Filtered {
            self.hits.clear();
            self.visible.clear();
            self.query.clear();
            self.scroll = 0;
            self.search_reveal = true;
            self.rebuild();
        }
    }
}
