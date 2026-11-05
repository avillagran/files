//! Toolkit popup adapter. Actions remain Files actions; no filesystem work here.
use crate::{
    model::{Action, Model},
    painter::Rect,
};
use files_core::theme::{Corners, WindowMode};
use fltk::{frame::Frame, prelude::*};
use fltk_omarchy::popups::{Dismiss, PopupHandle, PopupItem, PopupMenu};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

/// Internal FLTK regression input, called only by the opt-in popup fixture.
/// Separate phases preserve FLTK's pushed widget across asynchronous work.
pub fn fixture_right_button(window: &fltk::window::Window, x: i32, y: i32, pressed: bool) {
    unsafe extern "C" {
        #[link_name = "_ZN2Fl3e_xE"]
        static mut X: i32;
        #[link_name = "_ZN2Fl3e_yE"]
        static mut Y: i32;
        #[link_name = "_ZN2Fl8e_x_rootE"]
        static mut ROOT_X: i32;
        #[link_name = "_ZN2Fl8e_y_rootE"]
        static mut ROOT_Y: i32;
        #[link_name = "_ZN2Fl8e_clicksE"]
        static mut CLICKS: i32;
        #[link_name = "_ZN2Fl8e_keysymE"]
        static mut KEYSYM: i32;
        #[link_name = "_ZN2Fl7e_stateE"]
        static mut STATE: i32;
    }
    // UI thread only, same event globals as the toolkit's focus::click_at.
    unsafe {
        X = x;
        Y = y;
        ROOT_X = window.x() + x;
        ROOT_Y = window.y() + y;
        CLICKS = 0;
        KEYSYM = 0xfee8 + 3;
        STATE = if pressed { 0x0400_0000 } else { 0 };
    }
    let _ = fltk::app::handle(
        if pressed {
            fltk::enums::Event::Push
        } else {
            fltk::enums::Event::Released
        },
        window,
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Settings,
    Actions,
    Context,
}
struct Active {
    /// Flat context-menu index chosen in any submenu (`on_pick` runs before
    /// `on_dismiss`, whose index is only the top-level row).
    picked: Rc<RefCell<Option<String>>>,
    kind: Kind,
    generation: u64,
    owner: (u64, u64),
    actions: Vec<Option<Action>>,
    handle: PopupHandle,
}
#[derive(Default)]
pub struct Manager {
    active: Option<Active>,
    dismissed: Rc<RefCell<VecDeque<Dismiss>>>,
}
fn row(label: &str, shortcut: &str, checked: bool) -> PopupItem {
    PopupItem::new("", label)
        .shortcut(shortcut)
        .checked(checked)
}
fn toolbar(m: &Model, kind: Kind) -> (Vec<PopupItem>, Vec<Option<Action>>) {
    let entries: Vec<(PopupItem, Action)> = match kind {
        Kind::Settings => vec![
            (
                row(" Guardar vista de esta carpeta", "", false),
                Action::SaveFolderView,
            ),
            (
                row(" Olvidar vista de esta carpeta", "", false),
                Action::ForgetFolderView,
            ),
            (
                row(" Recargar/reintentar vista", "", false),
                Action::RetryFolderView,
            ),
            (
                row(" Omitir intento de vista", "", false),
                Action::DiscardFolderView,
            ),
            (
                row("  Barra de operaciones MC", "Ctrl+Alt+B", m.operation_bar),
                Action::ToggleOperationBar,
            ),
            (
                row("Ventana: Normal", "", m.window_mode == WindowMode::Normal),
                Action::WindowMode(WindowMode::Normal),
            ),
            (
                row(
                    "Ventana: Pantalla completa",
                    "",
                    m.window_mode == WindowMode::Fullscreen,
                ),
                Action::WindowMode(WindowMode::Fullscreen),
            ),
            (
                row("  Mostrar ocultos", "Ctrl+H", m.show_hidden),
                Action::ToggleHidden,
            ),
            (
                row("  Doble panel", "F9", m.dual.enabled),
                Action::DualPane,
            ),
            (
                row("Bordes: Omarchy", "", m.corners == Corners::Omarchy),
                Action::Corner(Corners::Omarchy),
            ),
            (
                row("Bordes: Cuadrados", "", m.corners == Corners::Square),
                Action::Corner(Corners::Square),
            ),
            (
                row("Bordes: Redondos", "", m.corners == Corners::Rounded),
                Action::Corner(Corners::Rounded),
            ),
            (
                row("Tema: Sistema", "", !m.alternate_theme),
                Action::Theme(false),
            ),
            (
                row("Tema: Tokyo Night", "", m.alternate_theme),
                Action::Theme(true),
            ),
            (
                row(
                    if m.dual.enabled {
                        "  Guardar sólo este grupo"
                    } else {
                        "  Guardar sesión"
                    },
                    "",
                    false,
                ),
                Action::SaveSession,
            ),
            (row("  Ayuda de atajos", "F1", false), Action::Help),
        ],
        Kind::Actions => vec![
            (
                row(" SHA-256 · comparar archivos", "", false),
                Action::Checksums,
            ),
            (
                row(" Duplicados en esta carpeta", "", false),
                Action::Duplicates,
            ),
            (
                row(" Favoritos y lugares", "Ctrl+Alt+D", false),
                Action::Bookmarks,
            ),
            (
                row(" Añadir carpeta a favoritos", "Ctrl+D", false),
                Action::BookmarkAdd,
            ),
            (
                row("  Gestor de tamaños", "Ctrl+Alt+Space", false),
                Action::Sizes,
            ),
            (row("  SFTP · montaje", "", false), Action::Sftp),
            (
                row("󰆧 Crear ZIP: revisar", "", false),
                Action::CreateArchive,
            ),
            (
                row("󰇚  Extraer ZIP: revisar", "", false),
                Action::ExtractArchive,
            ),
            (
                row("  Buscar comandos", "Ctrl+Shift+P", false),
                Action::Commands,
            ),
            (
                row("󰄬  Marcar filtrados y mostrar todo", "Ctrl+Shift+A", false),
                Action::Selection(crate::selection_tools::Command::Filtered),
            ),
            (
                row("󰄲  Seleccionar todo", "Ctrl+A", false),
                Action::Selection(crate::selection_tools::Command::All),
            ),
            (
                row("󰒟  Invertir selección", "Ctrl+I", false),
                Action::Selection(crate::selection_tools::Command::Invert),
            ),
            (
                row("󰅖  Limpiar selección", "Ctrl+Shift+D", false),
                Action::Selection(crate::selection_tools::Command::Clear),
            ),
            (
                row(
                    if m.sizes.has_work() {
                        "  Parar tamaños"
                    } else {
                        "  Carpetas listadas"
                    },
                    "Ctrl+Shift+Space",
                    false,
                ),
                if m.sizes.has_work() {
                    Action::CancelListedSizes
                } else {
                    Action::CalculateListedSizes
                },
            ),
            (
                row(
                    if m.sizes.paused() {
                        "  Reanudar tamaños"
                    } else {
                        "  Pausar siguientes tamaños"
                    },
                    "",
                    false,
                ),
                Action::PauseSizes(!m.sizes.paused()),
            ),
            (
                row("  Reintentar tamaños fallidos", "", false),
                Action::RetrySizeErrors,
            ),
            (
                row("  Buscar en subcarpetas", "Ctrl+Shift+F", false),
                Action::Search,
            ),
            (row("  Filtrar carpeta", "Ctrl+F", false), Action::Filter),
            (
                row("  Calcular tamaño", "Ctrl+Space", false),
                Action::CalculateSize,
            ),
            (row("  Vista previa", "Space", false), Action::Preview),
            (
                row("  Propiedades", "Alt+Enter", false),
                Action::Properties,
            ),
            (
                row("  Copiar en files", "Ctrl+C", false),
                Action::ClipboardCopy,
            ),
            (
                row("  Movimientos interrumpidos", "", false),
                Action::RecoveryCatalog,
            ),
            (
                row("  Cortar en files", "Ctrl+X", false),
                Action::ClipboardCut,
            ),
            (row("  Renombrar", "F2", false), Action::Rename),
            (row("  Enviar a Papelera", "Supr", false), Action::Trash),
            (
                row("  Crear archivo", "Ctrl+N", false),
                Action::NewEntry(false),
            ),
            (row("  Crear carpeta", "F7", false), Action::NewEntry(true)),
            (row("  Deshacer", "Ctrl+Z", false), Action::Undo(false)),
            (row("  Rehacer", "Ctrl+Shift+Z", false), Action::Undo(true)),
            (
                row("  Pegar en files", "Ctrl+V", false),
                Action::ClipboardPaste,
            ),
        ],
        Kind::Context => unreachable!(),
    };
    entries
        .into_iter()
        .map(|(item, action)| (item, Some(action)))
        .unzip()
}
impl Manager {
    pub fn close(&self) {
        if let Some(active) = &self.active {
            active.handle.close();
        }
    }
    pub fn sync(
        &mut self,
        m: &mut Model,
        canvas: &Frame,
        settings_anchor: &mut Frame,
        actions_anchor: &mut Frame,
    ) -> bool {
        let mut dirty = false;
        loop {
            let reason = { self.dismissed.borrow_mut().pop_front() };
            let Some(reason) = reason else { break };
            if let Some(active) = self.active.take() {
                m.toolkit_popup = false;
                let valid = active.generation == m.generation
                    && active.owner == m.pane_owner()
                    && !m.closing
                    && !m.search.visible
                    && m.bookmarks.panel.is_none()
                    && m.commands.is_none()
                    && m.size_panel.is_none();
                let valid = valid && m.trash_browser.is_none();
                if let Dismiss::Picked(index) = reason
                    && valid
                {
                    if active.kind == Kind::Context {
                        if let Some(flat) = active
                            .picked
                            .borrow_mut()
                            .take()
                            .and_then(|id| id.parse::<usize>().ok())
                        {
                            if let Some(menu) = m.context_menu.as_ref() {
                                eprintln!(
                                    "FILES_TOOLKIT_CONTEXT_PICK flat={flat} label={}",
                                    menu.items.get(flat).map_or("", |i| &i.label)
                                );
                            }
                            crate::context_menu::activate(m, flat);
                        }
                    } else if let Some(Some(action)) = active.actions.get(index) {
                        m.action(action.clone());
                    }
                }
                match active.kind {
                    Kind::Settings => m.settings = false,
                    Kind::Actions => m.actions_menu = false,
                    Kind::Context => m.context_menu = None,
                }
                m.hits.clear();
                let focus_returned = fltk::app::focus()
                    .is_some_and(|widget| widget.as_widget_ptr() == canvas.as_widget_ptr());
                eprintln!(
                    "FILES_TOOLKIT_POPUP_DISMISS kind={:?} reason={reason:?} valid={valid} focus_returned={focus_returned} rename_open={}",
                    active.kind,
                    m.rename_dialog.is_some()
                );
                dirty = true;
            }
        }
        if let Some(active) = &self.active {
            let requested = match active.kind {
                Kind::Settings => m.settings,
                Kind::Actions => m.actions_menu,
                Kind::Context => m.context_menu.is_some(),
            };
            if !requested
                || m.closing
                || active.generation != m.generation
                || active.owner != m.pane_owner()
            {
                active.handle.close();
            }
            return dirty;
        }
        let kind = if m.context_menu.is_some() {
            Kind::Context
        } else if m.settings {
            Kind::Settings
        } else if m.actions_menu {
            Kind::Actions
        } else {
            return dirty;
        };
        let (items, actions, position) = if kind == Kind::Context {
            let menu = m.context_menu.as_ref().expect("context menu");
            let items = context_items(menu);
            (items, Vec::new(), Some(menu.anchor))
        } else {
            let (items, actions) = toolbar(m, kind);
            (items, actions, None)
        };
        let anchor_rect = if kind == Kind::Context {
            None
        } else {
            m.hits.iter().rev().find_map(|hit| {
                let matches = matches!(
                    (kind, &hit.action),
                    (Kind::Settings, Action::Settings) | (Kind::Actions, Action::Actions)
                );
                (matches && hit.generation == m.generation).then_some(hit.rect)
            })
        };
        if kind != Kind::Context && anchor_rect.is_none() {
            return dirty;
        }
        let queue = self.dismissed.clone();
        let picked: Rc<RefCell<Option<String>>> = Rc::default();
        let pick = picked.clone();
        let handle = if let Some((x, y)) = position {
            PopupMenu::open_at_id(canvas, x, y, items, move |id| {
                *pick.borrow_mut() = Some(id.to_string());
            })
        } else {
            let Rect { x, y, w, h } = anchor_rect.expect("toolbar anchor");
            let anchor = if kind == Kind::Settings {
                settings_anchor
            } else {
                actions_anchor
            };
            anchor.resize(x, y, w, h);
            PopupMenu::open(anchor, items, |_| {})
        }
        .on_dismiss(move |reason| queue.borrow_mut().push_back(reason));
        if !handle.is_open() {
            return dirty;
        }
        eprintln!(
            "FILES_TOOLKIT_POPUP_OPEN kind={kind:?} rect={:?}",
            handle.rect()
        );
        m.toolkit_popup = true;
        self.active = Some(Active {
            picked,
            kind,
            generation: m.generation,
            owner: m.pane_owner(),
            actions,
            handle,
        });
        true
    }
}

/// Toolkit rows for a context menu: the most used entries at the root and
/// the rest in submenus ([`crate::context_menu::place`]). Every leaf id is
/// its flat index in `menu.items`, so activation stays index-based.
pub fn context_items(menu: &crate::context_menu::Menu) -> Vec<PopupItem> {
    use crate::context_menu::Placed;
    let leaf = |index: usize| {
        let i = &menu.items[index];
        let (icon, text) = split_icon(&i.label);
        let item = match text.rsplit_once(" · ") {
            Some((label, hint)) => PopupItem::new(icon, label).shortcut(hint),
            None => PopupItem::new(icon, text),
        }
        .id(&index.to_string());
        if i.action.is_some() {
            item
        } else {
            item.disabled()
        }
    };
    let mut rows: Vec<PopupItem> = Vec::new();
    for placed in crate::context_menu::place(menu) {
        match placed {
            Placed::Item(index) => rows.push(leaf(index)),
            Placed::Group(icon, label, children) => {
                let enabled = children.iter().any(|&i| menu.items[i].action.is_some());
                let group =
                    PopupItem::submenu(icon, label, children.into_iter().map(leaf).collect());
                rows.push(if enabled { group } else { group.disabled() });
            }
            Placed::Divider => {
                if let Some(last) = rows.pop() {
                    rows.push(last.divider());
                }
            }
        }
    }
    rows
}

/// Model labels start with a Nerd Font glyph ("\u{f0c5}  Copiar"): it goes to
/// the toolkit icon column so leaves and submenu rows line up.
fn split_icon(label: &str) -> (&str, &str) {
    let mut chars = label.char_indices();
    match chars.next() {
        Some((_, c)) if matches!(c, '\u{e000}'..='\u{f8ff}' | '\u{f0000}'..) => {
            let rest = &label[c.len_utf8()..];
            (&label[..c.len_utf8()], rest.trim_start())
        }
        _ => ("", label.trim_start()),
    }
}
/// Keyboard route to a flat context item in [`context_items`]: its root row,
/// the Down presses from the first enabled root row, and inside a submenu
/// the Down presses from its first enabled child (Right opens it there).
pub fn context_route(
    menu: &crate::context_menu::Menu,
    flat: usize,
) -> Option<(usize, usize, Option<usize>)> {
    let id = flat.to_string();
    let rows = context_items(menu);
    let root = rows
        .iter()
        .position(|r| r.id == id || r.children.iter().any(|c| c.id == id))?;
    let downs = rows[..root].iter().filter(|r| r.enabled).count();
    let child = rows[root].is_submenu().then(|| {
        rows[root]
            .children
            .iter()
            .take_while(|c| c.id != id)
            .filter(|c| c.enabled)
            .count()
    });
    Some((root, downs, child))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nerd_glyph_moves_to_the_icon_column() {
        assert_eq!(
            split_icon("\u{f0c5}  Copiar · Ctrl+C"),
            ("\u{f0c5}", "Copiar · Ctrl+C")
        );
        assert_eq!(
            split_icon("\u{f0001} Crear ZIP"),
            ("\u{f0001}", "Crear ZIP")
        );
        assert_eq!(
            split_icon("Comparar esta carpeta…"),
            ("", "Comparar esta carpeta…")
        );
        assert_eq!(split_icon(""), ("", ""));
    }
    #[test]
    fn toolbar_keeps_preferences_and_file_actions_separate() {
        let m = Model::new("/fixture-only".into());
        let (preferences, preference_actions) = toolbar(&m, Kind::Settings);
        let (actions, file_actions) = toolbar(&m, Kind::Actions);
        assert!(
            preferences
                .iter()
                .any(|item| item.label.contains("Mostrar ocultos") && item.shortcut == "Ctrl+H")
        );
        assert!(
            preference_actions
                .iter()
                .any(|a| matches!(a, Some(Action::WindowMode(WindowMode::Normal))))
        );
        assert!(
            !preference_actions
                .iter()
                .any(|a| matches!(a, Some(Action::Trash)))
        );
        assert!(
            actions
                .iter()
                .any(|item| item.label.contains("Renombrar") && item.shortcut == "F2")
        );
        assert!(
            file_actions
                .iter()
                .any(|a| matches!(a, Some(Action::Trash)))
        );
        assert!(
            !file_actions
                .iter()
                .any(|a| matches!(a, Some(Action::WindowMode(_))))
        );
    }
}
