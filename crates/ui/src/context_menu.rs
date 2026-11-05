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
    pub label: std::borrow::Cow<'static, str>,
    pub action: Option<Action>,
    /// Section title (never focused or activated).
    pub section: bool,
    /// The entry's action even while disabled: places it in the toolkit
    /// submenu layout ([`layout`]) whether or not it can run now.
    pub intent: Option<Action>,
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
            label: label.into(),
            action: if enabled { Some(action.clone()) } else { None },
            section: false,
            intent: Some(action),
        }
    };
    let header = |label: &'static str| Item {
        label: label.into(),
        action: None,
        section: true,
        intent: None,
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
            header("ABRIR"),
            item("  Abrir elemento enfocado", Action::Open, true),
            item(
                "󰘔  Abrir con… · Ctrl+Shift+O",
                Action::OpenWith,
                m.open_job.is_none(),
            ),
            item("  Vista previa · Space", Action::Preview, true),
            item(
                "  Propiedades · Alt+Enter",
                Action::Properties,
                !busy && m.selection.len() <= 1,
            ),
            item(
                "  Terminal aquí · Ctrl+.",
                Action::Terminal,
                m.open_job.is_none(),
            ),
            header("EDITAR"),
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
                "  Pegar · Ctrl+V (lo último copiado)",
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
            item("  Copiar ruta · Ctrl+Alt+C", Action::CopyPath, true),
            header("ORGANIZAR"),
            item("  Renombrar · F2", Action::Rename, !busy),
            item("  Copiar selección · F5", Action::CopyFiles, !busy),
            item("  Mover selección · F6", Action::MoveFiles, !busy),
            item(
                "\u{f07b}  Nueva carpeta con la selección",
                Action::FolderWithSelection,
                !busy && !m.delete.busy() && !m.selection.is_empty(),
            ),
            item(
                "  Crear enlace · Ctrl+Shift+M",
                Action::CreateLinks,
                !busy && !m.delete.busy(),
            ),
            header("COMPRIMIR"),
            item("󰆧 Crear ZIP: revisar", Action::CreateArchive, !busy),
            item("󰇚  Extraer ZIP: revisar", Action::ExtractArchive, !busy),
            header("ELIMINAR"),
            item("  Papelera · Supr", Action::Trash, !busy),
            item(
                "󰆴  Eliminar permanentemente · Shift+Supr",
                Action::DeletePermanently,
                !busy && !m.delete.busy(),
            ),
            header("SELECCIÓN"),
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
        ],
        Target::Background => vec![
            header("CREAR"),
            item("  Crear carpeta · F7", Action::NewEntry(true), !busy),
            item("  Crear archivo · Ctrl+N", Action::NewEntry(false), !busy),
            header("PEGAR"),
            item(
                "  Pegar · Ctrl+V (lo último copiado)",
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
                "\u{f0c1}  Enlace a lo copiado · Ctrl+M",
                Action::LinkCopied,
                !busy && !m.delete.busy() && m.clipboard_state.count() > 0,
            ),
            header("VER"),
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
            header("SELECCIÓN"),
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
                "\u{f0b0}  Seleccionar por patrón · Ctrl+S",
                Action::SelectPattern,
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
            header("HERRAMIENTAS"),
            item("  Buscar en subcarpetas", Action::Search, true),
            item("Comparar esta carpeta…", Action::CompareFolders, true),
            item("Preparar sincronización…", Action::ReviewSyncFolders, true),
            item(
                "  Terminal aquí · Ctrl+.",
                Action::Terminal,
                m.open_job.is_none(),
            ),
        ],
    };
    let mut items = items;
    // Nautilus "New Document" from ~/Templates, inside the CREAR section.
    if target == Target::Background {
        let templates = files_core::quick_ops::templates(&files_core::quick_ops::templates_dir());
        let at = items
            .iter()
            .position(|i| i.section && i.label == "PEGAR")
            .unwrap_or(items.len());
        for (k, t) in templates.into_iter().take(8).enumerate() {
            let name = t
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            items.insert(
                at + k,
                Item {
                    label: format!("\u{f15b}  Nuevo: {name}").into(),
                    action: (!busy).then_some(Action::NewFromTemplate(t.clone())),
                    section: false,
                    intent: Some(Action::NewFromTemplate(t)),
                },
            );
        }
    }
    if target == Target::Background
        && let Some((unmount, eject, removable)) = crate::devices_ui::current_device_actions(m)
    {
        items.push(header("DISPOSITIVO"));
        items.push(item(
            "\u{f052}  Desmontar este dispositivo",
            unmount,
            !m.devices.busy(),
        ));
        if removable {
            items.push(item(
                "\u{f052}  Expulsar (retirar con seguridad)",
                eject,
                !m.devices.busy(),
            ));
        }
    }
    let first = items.iter().position(|i| i.action.is_some()).unwrap_or(0);
    let mut menu = Menu {
        target,
        items,
        focused: first,
        generation: m.generation,
        rect: Rect::new(x, y, 0, 0),
        anchor: (x, y),
        scroll: 0,
        map: crate::selection_tools::map_stamp(m),
    };
    menu.fit(w, h);
    menu
}
/// One row of the toolkit (submenu) layout of a context menu. The flat
/// [`Menu::items`] stay the single source of truth: the layout only says
/// where each item goes, by its action, and picks report flat indices.
pub enum Node {
    Leaf(fn(&Action) -> bool),
    Sub(&'static str, &'static str, &'static [fn(&Action) -> bool]),
    Divider,
}
/// The most used entries stay at the root; the rest is grouped by kind.
/// `None` keeps the flat list (short menus: views, selection).
pub fn layout(target: Target) -> Option<&'static [Node]> {
    use crate::selection_tools::Command;
    use Node::{Divider, Leaf, Sub};
    const ENTRIES: &[Node] = &[
        Leaf(|a| matches!(a, Action::Open)),
        Leaf(|a| matches!(a, Action::OpenWith)),
        Leaf(|a| matches!(a, Action::Preview)),
        Divider,
        Leaf(|a| matches!(a, Action::ClipboardCut)),
        Leaf(|a| matches!(a, Action::ClipboardCopy)),
        Leaf(|a| matches!(a, Action::ClipboardPaste)),
        Divider,
        Leaf(|a| matches!(a, Action::Rename)),
        Leaf(|a| matches!(a, Action::Trash)),
        Divider,
        Sub(
            "\u{f0ea}",
            "Portapapeles",
            &[
                |a| matches!(a, Action::NativePaste),
                |a| matches!(a, Action::CopyPath),
            ],
        ),
        Sub(
            "\u{f07c}",
            "Organizar",
            &[
                |a| matches!(a, Action::CopyFiles),
                |a| matches!(a, Action::MoveFiles),
                |a| matches!(a, Action::FolderWithSelection),
                |a| matches!(a, Action::CreateLinks),
                |a| matches!(a, Action::DeletePermanently),
            ],
        ),
        Sub(
            "\u{f187}",
            "Comprimir",
            &[
                |a| matches!(a, Action::CreateArchive),
                |a| matches!(a, Action::ExtractArchive),
            ],
        ),
        Sub(
            "\u{f046}",
            "Selección",
            &[|a| matches!(a, Action::Selection(_))],
        ),
        Leaf(|a| matches!(a, Action::Terminal)),
        Divider,
        Leaf(|a| matches!(a, Action::Properties)),
    ];
    const BACKGROUND: &[Node] = &[
        Leaf(|a| matches!(a, Action::NewEntry(true))),
        Leaf(|a| matches!(a, Action::NewEntry(false))),
        Sub(
            "\u{f15b}",
            "Nuevo documento",
            &[|a| matches!(a, Action::NewFromTemplate(_))],
        ),
        Divider,
        Leaf(|a| matches!(a, Action::ClipboardPaste)),
        Sub(
            "\u{f0ea}",
            "Pegado especial",
            &[
                |a| matches!(a, Action::NativePaste),
                |a| matches!(a, Action::LinkCopied),
            ],
        ),
        Divider,
        Leaf(|a| matches!(a, Action::ToggleHidden)),
        Leaf(|a| matches!(a, Action::Refresh)),
        Divider,
        Sub(
            "\u{f046}",
            "Seleccionar",
            &[
                |a| matches!(a, Action::Selection(Command::Filtered | Command::All)),
                |a| matches!(a, Action::SelectPattern),
                |a| matches!(a, Action::Selection(_)),
            ],
        ),
        Sub(
            "\u{f0ad}",
            "Herramientas",
            &[
                |a| matches!(a, Action::CalculateListedSizes | Action::CancelListedSizes),
                |a| matches!(a, Action::Search),
                |a| matches!(a, Action::CompareFolders),
                |a| matches!(a, Action::ReviewSyncFolders),
                |a| matches!(a, Action::Terminal),
            ],
        ),
        Sub(
            "\u{f052}",
            "Dispositivo",
            &[|a| matches!(a, Action::Device(..))],
        ),
    ];
    match target {
        Target::Entries => Some(ENTRIES),
        Target::Background => Some(BACKGROUND),
        Target::Views | Target::Selection => None,
    }
}
/// A placed layout row: a flat item index, or a titled group of indices.
#[derive(Debug, PartialEq)]
pub enum Placed {
    Item(usize),
    Group(&'static str, &'static str, Vec<usize>),
    Divider,
}
/// Places every non-section item exactly once. Items no layout rule claims
/// go to a final "Más" group, so a new entry can never vanish silently.
pub fn place(menu: &Menu) -> Vec<Placed> {
    let candidates: Vec<usize> = (0..menu.items.len())
        .filter(|&i| !menu.items[i].section)
        .collect();
    let Some(nodes) = layout(menu.target) else {
        return candidates.into_iter().map(Placed::Item).collect();
    };
    let mut used = vec![false; menu.items.len()];
    let mut take = |rule: &fn(&Action) -> bool, all: bool| {
        let mut found = Vec::new();
        for &i in &candidates {
            if !used[i] && menu.items[i].intent.as_ref().is_some_and(rule) {
                used[i] = true;
                found.push(i);
                if !all {
                    break;
                }
            }
        }
        found
    };
    let mut out = Vec::new();
    for node in nodes {
        match node {
            Node::Leaf(rule) => out.extend(take(rule, false).into_iter().map(Placed::Item)),
            Node::Sub(icon, label, rules) => {
                let mut children: Vec<usize> = rules.iter().flat_map(|r| take(r, true)).collect();
                children.sort_unstable();
                if !children.is_empty() {
                    out.push(Placed::Group(icon, label, children));
                }
            }
            Node::Divider => {
                if !matches!(out.last(), None | Some(Placed::Divider)) {
                    out.push(Placed::Divider);
                }
            }
        }
    }
    let rest: Vec<usize> = candidates.into_iter().filter(|&i| !used[i]).collect();
    if !rest.is_empty() {
        out.push(Placed::Group("\u{f141}", "Más", rest));
    }
    while matches!(out.last(), Some(Placed::Divider)) {
        out.pop();
    }
    out
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
        if item.section {
            // Group title with a hairline, like the menus of Nautilus.
            if rank > 0 {
                p.rect(
                    Rect::new(row.x + 4, row.y + 4, (row.w - 8).max(0), 1),
                    c.selection,
                );
            }
            p.text(
                Rect::new(row.x + 8, row.y + 8, (row.w - 16).max(0), row.h - 8),
                &item.label,
                9,
                c.dark_foreground,
                false,
                false,
            );
            continue;
        }
        if rank == menu.focused {
            p.rounded_rect(row, 4 * m.radius, c.selection);
        }
        p.text(
            Rect::new(row.x + 8, row.y, (row.w - 16).max(0), row.h),
            &item.label,
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

#[cfg(test)]
mod tests {
    use super::*;
    fn flat(menu: &Menu) -> Vec<usize> {
        let mut seen: Vec<usize> = place(menu)
            .into_iter()
            .flat_map(|p| match p {
                Placed::Item(i) => vec![i],
                Placed::Group(_, _, c) => c,
                Placed::Divider => vec![],
            })
            .collect();
        seen.sort_unstable();
        seen
    }
    #[test]
    fn submenu_layout_places_every_entry_once_and_keeps_the_root_short() {
        let mut m = Model::new("/fixture-only".into());
        m.query = "x".into();
        for target in [Target::Entries, Target::Background] {
            let mut menu = build(&m, target, 10, 10, 800, 600);
            // The background menu of a mounted device adds this entry.
            if target == Target::Background {
                menu.items.push(Item {
                    label: "\u{f052}  Desmontar este dispositivo".into(),
                    action: None,
                    section: false,
                    intent: Some(Action::Device("/dev/sdz1".into(), None)),
                });
            }
            let expected: Vec<usize> = (0..menu.items.len())
                .filter(|&i| !menu.items[i].section)
                .collect();
            assert_eq!(flat(&menu), expected, "{target:?}");
            let placed = place(&menu);
            assert!(
                !placed
                    .iter()
                    .any(|p| matches!(p, Placed::Group(_, "Más", _))),
                "{target:?}: an entry has no submenu rule"
            );
            let root = placed
                .iter()
                .filter(|p| !matches!(p, Placed::Divider))
                .count();
            assert!(root <= 14, "{target:?}: {root} root rows");
            assert!(!matches!(placed.first(), Some(Placed::Divider)));
            assert!(!matches!(placed.last(), Some(Placed::Divider)));
        }
        // Rename and Trash stay one keystroke away at the root.
        let menu = build(&m, Target::Entries, 10, 10, 800, 600);
        let root: Vec<_> = place(&menu)
            .into_iter()
            .filter_map(|p| match p {
                Placed::Item(i) => menu.items[i].intent.clone(),
                _ => None,
            })
            .collect();
        assert!(root.iter().any(|a| matches!(a, Action::Rename)));
        assert!(root.iter().any(|a| matches!(a, Action::Trash)));
        assert!(root.iter().any(|a| matches!(a, Action::ClipboardCopy)));
    }
    #[test]
    fn unknown_entries_fall_into_more_instead_of_vanishing() {
        let m = Model::new("/fixture-only".into());
        let mut menu = build(&m, Target::Entries, 10, 10, 800, 600);
        menu.items.push(Item {
            label: "Nueva".into(),
            action: Some(Action::Help),
            section: false,
            intent: Some(Action::Help),
        });
        let last = menu.items.len() - 1;
        assert_eq!(
            place(&menu).last(),
            Some(&Placed::Group("\u{f141}", "Más", vec![last]))
        );
    }
}
