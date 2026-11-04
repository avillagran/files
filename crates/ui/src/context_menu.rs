//! File/background context menu. Uses existing Actions; never performs I/O here.
use crate::{
    model::{Action, Hit, Model},
    painter::{Painter, Rect},
};
use fltk::enums::Key;
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    Entries,
    Background,
    Selection,
    Views,
}
pub struct Item {
    pub label: &'static str,
    pub action: Option<Action>,
}
pub struct Menu {
    pub target: Target,
    pub items: Vec<Item>,
    pub focused: usize,
    pub generation: u64,
    pub rect: Rect,
    pub anchor: (i32, i32),
    scroll: usize,
    map: Option<(u64, bool)>,
}
impl Menu {
    fn fit(&mut self, w: i32, h: i32) {
        let margin = if w >= 32 && h >= 32 { 8 } else { 0 };
        self.rect.w = 300.min((w - 2 * margin).max(0));
        self.rect.h = (32 + self.items.len() as i32 * 30).min((h - 2 * margin).max(0));
        self.rect.x = self
            .rect
            .x
            .clamp(margin, (w - self.rect.w - margin).max(margin));
        self.rect.y = self
            .rect
            .y
            .clamp(margin, (h - self.rect.h - margin).max(margin));
        let rows = ((self.rect.h - 32).max(0) / 30).max(1) as usize;
        if self.focused < self.scroll {
            self.scroll = self.focused;
        }
        if self.focused >= self.scroll + rows {
            self.scroll = self.focused + 1 - rows;
        }
    }
}
fn build(m: &Model, target: Target, x: i32, y: i32, w: i32, h: i32) -> Menu {
    let busy = m.mutations.busy || m.mutation_applied || m.copy_busy();
    let item = |label, action, enabled| {
        let mut label = label;
        let mut enabled = enabled;
        if crate::copy_review::supported(&action) {
            enabled = crate::copy_review::reason(m, &action).is_none();
        }
        if crate::selection_tools::is_map(m) && crate::map_actions::supported(&action) {
            enabled = crate::map_actions::reason(m, &action).is_none();
        }
        if matches!(action, Action::CopyFiles) && crate::selection_tools::is_map(m) {
            label = "  Copiar selección: revisar destino";
        }
        if let Action::Selection(command) = &action
            && crate::selection_tools::is_map(m)
        {
            let reason = crate::selection_tools::map_reason(m, *command);
            label = match command {
                crate::selection_tools::Command::Filtered => "Filtro no aplicado en Espacio",
                crate::selection_tools::Command::All => {
                    if reason.is_some() {
                        "Espera la proyección de Espacio"
                    } else {
                        "Marcar objetos del mapa · Ctrl+A"
                    }
                }
                crate::selection_tools::Command::Invert => {
                    if reason.is_some() {
                        "Espera la proyección de Espacio"
                    } else {
                        "Invertir marcas del mapa · Ctrl+I"
                    }
                }
                _ => label,
            };
            enabled &= reason.is_none();
        }
        Item {
            label,
            action: if enabled { Some(action) } else { None },
        }
    };
    let items = match target {
        Target::Views => vec![
            item(
                "  Grupos · Ctrl+1",
                Action::View(crate::model::View::Groups),
                true,
            ),
            item(
                "  Lista · Ctrl+2",
                Action::View(crate::model::View::List),
                true,
            ),
            item(
                "  Columnas · Ctrl+3",
                Action::View(crate::model::View::Columns),
                true,
            ),
            item(
                "  Espacio · Ctrl+4",
                Action::View(crate::model::View::SpaceMap),
                true,
            ),
            item(
                "  Grilla · Ctrl+5",
                Action::View(crate::model::View::Grid),
                true,
            ),
            item("  Doble panel · F9", Action::DualPane, true),
        ],
        Target::Selection => vec![
            item(
                "󰄬  Marcar filtrados y mostrar todo",
                Action::Selection(crate::selection_tools::Command::Filtered),
                !m.query.trim().is_empty() && !m.selection_refreshing(),
            ),
            item(
                "󰄲  Seleccionar todo · Ctrl+A",
                Action::Selection(crate::selection_tools::Command::All),
                !m.selection_refreshing(),
            ),
            item(
                "󰒟  Invertir selección · Ctrl+I",
                Action::Selection(crate::selection_tools::Command::Invert),
                !m.selection_refreshing(),
            ),
            item(
                "󰅖  Limpiar · Ctrl+Shift+D",
                Action::Selection(crate::selection_tools::Command::Clear),
                !m.selection_refreshing(),
            ),
        ],
        Target::Entries => vec![
            item(
                "󰒟  Invertir selección · Ctrl+I",
                Action::Selection(crate::selection_tools::Command::Invert),
                !m.selection_refreshing(),
            ),
            item(
                "󰅖  Limpiar selección",
                Action::Selection(crate::selection_tools::Command::Clear),
                !m.selection_refreshing(),
            ),
            item("  Abrir elemento enfocado", Action::Open, true),
            item("  Vista previa · Space", Action::Preview, true),
            item("  Renombrar · F2", Action::Rename, !busy),
            item("  Copiar selección · F5", Action::CopyFiles, !busy),
            item("  Mover selección · F6", Action::MoveFiles, !busy),
            item("󰇚  Extraer ZIP: revisar", Action::ExtractArchive, !busy),
            item("󰆧 Crear ZIP: revisar", Action::CreateArchive, !busy),
            item("  Papelera · Supr", Action::Trash, !busy),
            item(
                "  Propiedades · Alt+Enter",
                Action::Properties,
                !busy && m.selection.len() <= 1,
            ),
            item(
                "  Copiar · Ctrl+C (files)",
                Action::ClipboardCopy,
                !m.mutations.busy && !m.mutation_applied,
            ),
            item(
                "  Cortar · Ctrl+X (files)",
                Action::ClipboardCut,
                !m.mutations.busy && !m.mutation_applied,
            ),
            item(
                "  Pegar en files · Ctrl+V",
                Action::ClipboardPaste,
                !m.mutations.busy
                    && !m.mutation_applied
                    && m.clipboard_state.count() > 0
                    && !m.clipboard_state.busy(),
            ),
            item(
                "  Pegar del sistema · Ctrl+Shift+V",
                Action::NativePaste,
                !m.mutations.busy && !m.mutation_applied,
            ),
            item(
                "  Terminal en esta carpeta",
                Action::Terminal,
                m.open_job.is_none(),
            ),
        ],
        Target::Background => vec![
            item("Comparar esta carpeta…", Action::CompareFolders, true),
            item("Preparar sincronización…", Action::ReviewSyncFolders, true),
            item(
                "󰄬  Marcar filtrados y mostrar todo",
                Action::Selection(crate::selection_tools::Command::Filtered),
                !m.query.trim().is_empty() && !m.selection_refreshing(),
            ),
            item(
                "󰄲  Seleccionar todo · Ctrl+A",
                Action::Selection(crate::selection_tools::Command::All),
                !m.selection_refreshing(),
            ),
            item(
                "󰒟  Invertir selección · Ctrl+I",
                Action::Selection(crate::selection_tools::Command::Invert),
                !m.selection_refreshing(),
            ),
            item(
                "󰅖  Limpiar selección",
                Action::Selection(crate::selection_tools::Command::Clear),
                !m.selection_refreshing(),
            ),
            item("  Crear archivo · Ctrl+N", Action::NewEntry(false), !busy),
            item("  Crear carpeta · F7", Action::NewEntry(true), !busy),
            item(
                if m.show_hidden {
                    "  Ocultar ocultos · Ctrl+H"
                } else {
                    "  Mostrar ocultos · Ctrl+H"
                },
                Action::ToggleHidden,
                !busy,
            ),
            item("  Actualizar · Ctrl+R", Action::Refresh, true),
            item(
                if m.sizes.has_work() {
                    "  Parar tamaños"
                } else {
                    "  Tamaños del listado"
                },
                if m.sizes.has_work() {
                    Action::CancelListedSizes
                } else {
                    Action::CalculateListedSizes
                },
                true,
            ),
            item("  Buscar en subcarpetas", Action::Search, true),
            item(
                "  Terminal en esta carpeta",
                Action::Terminal,
                m.open_job.is_none(),
            ),
            item(
                "  Pegar en files · Ctrl+V",
                Action::ClipboardPaste,
                !m.mutations.busy
                    && !m.mutation_applied
                    && m.clipboard_state.count() > 0
                    && !m.clipboard_state.busy(),
            ),
            item(
                "  Pegar del sistema · Ctrl+Shift+V",
                Action::NativePaste,
                !m.mutations.busy && !m.mutation_applied,
            ),
        ],
    };
    let mut menu = Menu {
        target,
        items,
        focused: 0,
        generation: m.generation,
        rect: Rect::new(x, y, 0, 0),
        anchor: (x, y),
        scroll: 0,
        map: crate::selection_tools::map_stamp(m),
    };
    menu.fit(w, h);
    menu
}
fn available(m: &Model) -> bool {
    !m.closing
        && m.comparison.is_none()
        && m.bookmarks.panel.is_none()
        && m.sftp.form.is_none()
        && m.size_panel.is_none()
        && !m.search.visible
        && m.help.is_none()
        && !m.preview.visible()
        && m.properties_dialog.is_none()
        && m.rename_dialog.is_none()
        && m.copy_form.is_none()
        && !m.notice_open
        && !m.mutation_applied
        && m.drag.is_none()
}
pub fn open_at(m: &mut Model, x: i32, y: i32, w: i32, h: i32) -> bool {
    if !available(m) {
        return false;
    }
    let action = m
        .hits
        .iter()
        .rev()
        .find(|hit| hit.generation == m.generation && hit.rect.contains(x, y))
        .map(|hit| hit.action.clone());
    let action = match action {
        Some(Action::PaneEntry(id, rev, i)) => {
            if id != m.dual.active_id {
                let Some(other) = m
                    .dual
                    .other
                    .as_ref()
                    .filter(|b| b.id == id && b.revision == rev && b.order.contains(&i))
                else {
                    return false;
                };
                let keep = other.selection.contains(&i);
                if keep {
                    m.switch_dual();
                    if m.dual.active_id != id {
                        return false;
                    }
                    m.selected = Some(i);
                } else {
                    m.pane_select(id, rev, i, false, false);
                }
            } else if rev != m.pane_revision {
                return false;
            }
            if m.dual.active_id != id {
                return false;
            }
            Some(Action::Entry(i))
        }
        Some(Action::PaneBackground(id, rev)) => {
            if !m.pane_background(id, rev) {
                return false;
            }
            Some(Action::None)
        }
        Some(Action::PaneFocus(id)) => {
            if id != m.dual.active_id && m.dual.other.as_ref().is_none_or(|b| b.id != id) {
                return false;
            }
            if id != m.dual.active_id {
                m.switch_dual();
            }
            if id != m.dual.active_id {
                return false;
            }
            Some(Action::None)
        }
        other => other,
    };
    let action = if let Some(Action::ColumnEntry(id, i)) = action {
        let active = m.columns.columns.get(m.columns.active).map(|c| c.id);
        if active != Some(id) {
            let Some(column) = m.columns.columns.iter_mut().find(|c| c.id == id) else {
                return false;
            };
            let Some(entry) = column
                .data
                .entries
                .get(i)
                .filter(|e| crate::columns::accepts(e, m.show_hidden, &column.data.query))
            else {
                return false;
            };
            let name = entry.name.clone();
            let keep = column.data.selection.contains(&i);
            if keep {
                column.data.selected = Some(i);
            }
            m.column_focus(id, if keep { None } else { Some(i) }, false, false);
            if m.columns.columns.get(m.columns.active).map(|c| c.id) != Some(id) {
                return false;
            }
            if m.selection_refreshing() {
                m.columns.deferred_menu = Some(crate::columns::DeferredMenu {
                    id,
                    generation: m.generation,
                    name,
                    rect: Rect::new(x, y, w, h),
                });
                return true;
            }
        }
        if m.selection_refreshing() {
            return false;
        }
        Some(Action::Entry(i))
    } else {
        action
    };
    let target = match action {
        Some(Action::Entry(i)) => {
            if m.selection_refreshing()
                || !m
                    .entries
                    .get(i)
                    .is_some_and(|e| files_core::visibility::accepts(&e.name, m.show_hidden))
            {
                return false;
            }
            if !m.selection.contains(&i) {
                m.select_entry(i, false, false);
            } else {
                m.selected = Some(i);
            }
            Target::Entries
        }
        None | Some(Action::None) => {
            let left = if m.rail { 170 } else { 52 };
            let right = w - if !m.dual.enabled && m.inspector && w >= 800 {
                240
            } else {
                0
            };
            if x < left
                || x >= right
                || y < 90
                || y >= crate::operation_bar::browser_height(m, h) - 32
            {
                return false;
            }
            m.selection.clear();
            m.preserve_empty_selection = true;
            m.selected = None;
            m.anchor = None;
            m.preview.close();
            Target::Background
        }
        _ => return false,
    };
    m.settings = false;
    m.actions_menu = false;
    m.context_menu = Some(build(m, target, x, y, w, h));
    m.hits.clear();
    true
}
pub fn open_loaded_selection(m: &mut Model, x: i32, y: i32, w: i32, h: i32) -> bool {
    if !available(m) || !m.loaded || m.selection_refreshing() {
        return false;
    }
    m.sanitize_selection();
    if m.selected.is_none() {
        return false;
    }
    m.settings = false;
    m.actions_menu = false;
    m.context_menu = Some(build(m, Target::Entries, x, y, w, h));
    m.hits.clear();
    true
}
pub fn open_views(m: &mut Model, w: i32, h: i32) {
    if !available(m) {
        return;
    }
    m.settings = false;
    m.actions_menu = false;
    m.hits.clear();
    m.context_menu = Some(build(m, Target::Views, (w - 330).max(0), 58, w, h));
}
pub fn open_selection(m: &mut Model, w: i32, h: i32) {
    if !available(m) {
        return;
    }
    m.settings = false;
    m.actions_menu = false;
    m.hits.clear();
    m.context_menu = Some(build(m, Target::Selection, (w - 330).max(0), 58, w, h));
}
pub fn open_keyboard(m: &mut Model, w: i32, h: i32) -> bool {
    if !available(m) {
        return false;
    }
    let focused = m.selected.filter(|&i| {
        m.entries
            .get(i)
            .is_some_and(|e| files_core::visibility::accepts(&e.name, m.show_hidden))
    });
    if let Some(index) = focused {
        let r = m
            .hits
            .iter()
            .find(|h| {
                h.generation == m.generation
                    && match h.action {
                        Action::Entry(i) | Action::ColumnEntry(_, i) => i == index,
                        Action::PaneEntry(id, rev, i) => {
                            id == m.dual.active_id && rev == m.pane_revision && i == index
                        }
                        _ => false,
                    }
            })
            .map(|h| h.rect)
            .unwrap_or(Rect::new(70, 100, 0, 0));
        if !m.selection.contains(&index) {
            m.select_entry(index, false, false);
        }
        m.settings = false;
        m.actions_menu = false;
        m.context_menu = Some(build(
            m,
            Target::Entries,
            r.x + r.w / 2,
            r.y + r.h / 2,
            w,
            h,
        ));
        m.hits.clear();
        true
    } else if m.dual.enabled {
        let x = if m.dual.right_active {
            if m.rail { 186 } else { 68 }
        } else {
            0
        } + if m.dual.right_active {
            m.dual_width / 2 + 12
        } else {
            if m.rail { 198 } else { 80 }
        };
        m.selection.clear();
        m.selected = None;
        m.anchor = None;
        m.preserve_empty_selection = true;
        m.context_menu = Some(build(m, Target::Background, x, h - 45, w, h));
        m.hits.clear();
        true
    } else {
        open_at(m, if m.rail { 188 } else { 70 }, h - 45, w, h)
    }
}
pub fn dismiss(m: &mut Model) {
    m.context_menu = None;
    m.hits.clear();
}
pub fn activate(m: &mut Model, index: usize) {
    let Some(menu) = m.context_menu.take() else {
        return;
    };
    if menu.generation != m.generation {
        m.hits.clear();
        return;
    }
    let Some(action) = menu.items.get(index).and_then(|item| item.action.clone()) else {
        m.context_menu = Some(menu);
        return;
    };
    m.hits.clear();
    if matches!(action, Action::Selection(_)) && menu.map != crate::selection_tools::map_stamp(m) {
        m.error = "Selección: el mapa cambió; abre de nuevo el menú".into();
        return;
    }
    let marks_are_the_result = matches!(action, Action::Selection(_));
    m.action(action);
    if menu.target == Target::Background && !marks_are_the_result {
        m.selection.clear();
        m.preserve_empty_selection = true;
        m.selected = None;
        m.anchor = None;
    }
}
pub fn pointer(m: &mut Model, x: i32, y: i32, left: bool) {
    let Some(menu) = &m.context_menu else {
        return;
    };
    if !left || !menu.rect.contains(x, y) {
        dismiss(m);
        return;
    }
    m.pointer_down(x, y, false, false, false);
}
pub fn key(m: &mut Model, key: Key) {
    if key == Key::Escape || key == Key::Menu {
        dismiss(m);
        return;
    }
    let Some(menu) = &mut m.context_menu else {
        return;
    };
    if key == Key::Up || key == Key::Down {
        let step = if key == Key::Up {
            menu.items.len() - 1
        } else {
            1
        };
        for _ in 0..menu.items.len() {
            menu.focused = (menu.focused + step) % menu.items.len();
            if menu.items[menu.focused].action.is_some() {
                break;
            }
        }
    } else if key == Key::Enter {
        let index = menu.focused;
        activate(m, index);
    }
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, w: i32, h: i32) {
    let Some(mut menu) = m.context_menu.take() else {
        return;
    };
    if menu.generation != m.generation || !available(m) {
        return;
    }
    menu.fit(w, h);
    m.hits.clear();
    let c = m.palette.clone();
    let r = menu.rect;
    p.rounded_rect(r, 8 * m.radius, c.muted);
    p.rounded_rect(
        Rect::new(r.x + 1, r.y + 1, (r.w - 2).max(0), (r.h - 2).max(0)),
        8 * m.radius,
        c.darker_background,
    );
    p.clip(r);
    p.text(
        Rect::new(r.x + 12, r.y + 4, (r.w - 24).max(0), 22),
        &match menu.target {
            Target::Views => "VISTAS".into(),
            Target::Selection => "SELECCIÓN · VISIBLES".into(),
            Target::Background => "CARPETA · FONDO".into(),
            Target::Entries => format!("{} SELECCIONADOS", m.selection.len()),
        },
        10,
        c.dark_foreground,
        false,
        false,
    );
    for (rank, item) in menu.items.iter().enumerate().skip(menu.scroll) {
        let row = Rect::new(
            r.x + 6,
            r.y + 28 + (rank - menu.scroll) as i32 * 30,
            (r.w - 12).max(0),
            28,
        );
        if row.y + row.h > r.y + r.h - 4 {
            break;
        }
        if rank == menu.focused {
            p.rounded_rect(row, 4 * m.radius, c.selection);
        }
        p.text(
            Rect::new(row.x + 8, row.y, (row.w - 16).max(0), row.h),
            item.label,
            12,
            if item.action.is_some() {
                c.foreground
            } else {
                c.dark_foreground
            },
            false,
            false,
        );
        if item.action.is_some() {
            m.hits.push(Hit {
                generation: m.generation,
                rect: row,
                action: Action::ContextItem(rank),
            });
        }
    }
    p.unclip();
    m.context_menu = Some(menu);
}
