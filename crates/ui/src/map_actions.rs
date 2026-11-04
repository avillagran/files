//! Disponibilidad de acciones de archivos en Espacio; el informe no es autoridad.
use crate::model::{Action, Model};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Focus,
    Selection,
    One,
    Root,
}
fn scope(action: &Action) -> Option<Scope> {
    Some(match action {
        Action::Open | Action::Preview | Action::CopyPath => Scope::Focus,
        Action::CopyFiles
        | Action::MoveFiles
        | Action::Rename
        | Action::Trash
        | Action::CreateArchive
        | Action::ExtractArchive
        | Action::ClipboardCopy
        | Action::ClipboardCut => Scope::Selection,
        Action::Properties => Scope::One,
        Action::NewEntry(_) | Action::ClipboardPaste | Action::Undo(_) => Scope::Root,
        _ => return None,
    })
}
pub fn supported(action: &Action) -> bool {
    scope(action).is_some()
}
fn focus_valid(m: &Model) -> bool {
    m.selected
        .and_then(|i| m.entries.get(i))
        .is_some_and(|e| files_core::visibility::accepts(&e.name, m.show_hidden))
}
/// Consulta acotada para pintura; no recorre marcas ni consulta el sistema de archivos.
pub fn reason(m: &Model, action: &Action) -> Option<&'static str> {
    if !crate::selection_tools::is_map(m) {
        return None;
    }
    let scope = scope(action)?;
    if crate::copy_review::supported(action) {
        return crate::copy_review::reason(m, action);
    }
    if m.closing {
        return Some("Espacio se está cerrando");
    }
    if !m.loaded || m.selection_refreshing() {
        return Some("Espera la carga de la carpeta");
    }
    if m.editing || m.filter_edit.active {
        return Some("Enter: vuelve al explorador");
    }
    if m.mutation_applied {
        return Some("Espera la operación en curso");
    }
    if m.rename_dialog.is_some()
        || m.copy_form.is_some()
        || m.properties_dialog.is_some()
        || m.notice_open
        || m.recovery_catalog.open
        || m.preview.visible()
        || m.search.visible
        || m.help.is_some()
    {
        return Some("Cierra el diálogo o herramienta primero");
    }
    if matches!(
        action,
        Action::Rename | Action::Trash | Action::Properties | Action::NewEntry(_) | Action::Undo(_)
    ) && (m.mutations.busy || m.copy_busy())
    {
        return Some("Espera la operación en curso");
    }
    if matches!(
        action,
        Action::ClipboardCopy | Action::ClipboardCut | Action::ClipboardPaste
    ) && (m.mutations.busy || m.clipboard_state.busy())
    {
        return Some("Espera el buffer o la operación en curso");
    }
    match scope {
        Scope::Focus if !focus_valid(m) => return Some("Enfoca un elemento primero"),
        Scope::Selection | Scope::One if m.selection.is_empty() && !focus_valid(m) => {
            return Some("Selecciona al menos un elemento");
        }
        Scope::One if m.selection.len() > 1 => return Some("Propiedades admite un elemento"),
        _ => {}
    }
    if matches!(action, Action::Open) {
        if let Some(why) =
            crate::selection_tools::map_reason(m, crate::selection_tools::Command::All)
        {
            return Some(why);
        }
        if !m
            .selected
            .is_some_and(|i| m.space_geometry.contains_entry(i))
        {
            return Some("Enfoca un objeto del mapa vigente");
        }
    }
    let count = if m.selection.is_empty() {
        usize::from(focus_valid(m))
    } else {
        m.selection.len()
    };
    match action {
        Action::ClipboardCopy | Action::ClipboardCut if count > 128 => {
            Some("Selecciona hasta 128 raíces")
        }
        Action::Trash if count > crate::trash_history::MAX_ITEMS => {
            Some("Papelera admite hasta 64 elementos")
        }
        Action::ClipboardPaste if m.clipboard_state.count() == 0 => {
            Some("El buffer de files está vacío")
        }
        Action::Undo(false) if !m.can_undo => Some("No hay operación para deshacer"),
        Action::Undo(true) if !m.can_redo => Some("No hay renombrado para rehacer"),
        _ => None,
    }
}
/// Valida las marcas una vez al ejecutar, antes de limpiar o capturar selección.
pub fn validate(m: &Model, action: &Action) -> Option<&'static str> {
    if !crate::selection_tools::is_map(m) {
        return None;
    }
    if crate::copy_review::supported(action) {
        return crate::copy_review::validate(m, action);
    }
    if let Some(why) = reason(m, action) {
        return Some(why);
    }
    if matches!(scope(action), Some(Scope::Selection | Scope::One))
        && m.selection.iter().any(|&i| {
            !m.entries
                .get(i)
                .is_some_and(|e| files_core::visibility::accepts(&e.name, m.show_hidden))
        })
    {
        return Some("La selección cambió; vuelve a marcar los elementos");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use files_core::listing::{Entry, Kind};
    fn model() -> Model {
        let mut m = Model::new("/fixture-map-actions".into());
        m.view = crate::model::View::SpaceMap;
        m.loaded = true;
        m.operation_bar = true;
        m.entries = ["one.txt", "two.txt", ".hidden"]
            .into_iter()
            .map(|name| Entry {
                name: name.into(),
                kind: Kind::File,
                metadata: None,
                stat_attempted: false,
            })
            .collect();
        m.order = vec![0, 1];
        m.selected = Some(0);
        m.selection.extend([0, 1]);
        m.query = "ignorado-en-el-mapa".into();
        m
    }
    #[test]
    fn mc_move_and_palette_copy_capture_raw_marks_without_a_size_report() {
        let mut m = model();
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            m.entries[1].name = std::ffi::OsString::from_vec(b"raw\xff\n.txt".to_vec());
        }
        let expected = m.entries[..2]
            .iter()
            .map(|e| e.name.clone())
            .collect::<Vec<_>>();
        let owner = m.pane_owner();
        let generation = m.generation;
        m.operation_bar_run(owner, generation, 4);
        let form = m.copy_form.as_ref().unwrap();
        assert!(form.moving);
        assert_eq!(form.names.as_ref(), expected.as_slice());
        m.action(Action::CopyDialogCancel);
        m.open_commands();
        let i = crate::command_palette::ITEMS
            .iter()
            .position(|s| s.id == "copy")
            .unwrap();
        let state = m.commands.as_mut().unwrap();
        state.matches = vec![(i, 0)];
        state.selected = 0;
        m.run_command();
        let form = m.copy_form.as_ref().unwrap();
        assert!(!form.moving);
        assert_eq!(form.names.as_ref(), expected.as_slice());
        assert_eq!(m.query, "ignorado-en-el-mapa");
        assert_eq!(m.copy.queued_count(), 0);
    }
    #[test]
    fn invalid_or_hidden_marks_do_not_fall_back_to_a_different_focused_file() {
        let mut m = model();
        for bad in [2, 999] {
            m.selection.clear();
            m.selection.insert(bad);
            m.selected = Some(0);
            m.action(Action::CopyFiles);
            assert!(m.copy_form.is_none());
            assert!(m.error.contains("selección cambió"));
            assert_eq!(m.selection.iter().copied().collect::<Vec<_>>(), vec![bad]);
            assert_eq!(m.selected, Some(0));
        }
        m.action(Action::Selection(crate::selection_tools::Command::Clear));
        m.action(Action::CopyFiles);
        assert!(m.copy_form.is_none());
        assert!(m.error.contains("Selecciona"));
    }
    #[test]
    fn queued_copy_allows_another_review_but_blocks_rename_and_properties() {
        let mut m = model();
        m.copy
            .enqueue(
                crate::copy_job::Request::new(
                    1,
                    vec![files_core::copy_engine::CopyRequest {
                        source: "/fixture-map-actions/one.txt".into(),
                        destination: "/fixture-map-actions-copy/one.txt".into(),
                    }],
                    Default::default(),
                )
                .start_paused(),
            )
            .unwrap();
        assert!(m.copy_busy());
        assert!(reason(&m, &Action::CopyFiles).is_none());
        assert!(reason(&m, &Action::MoveFiles).is_none());
        assert!(reason(&m, &Action::Rename).is_some());
        assert!(reason(&m, &Action::Properties).is_some());
        m.action(Action::CopyFiles);
        assert!(m.copy_form.is_some());
        assert_eq!(m.copy.queued_count(), 1);
    }
    #[test]
    fn loading_closed_and_busy_guards_preserve_explicit_empty_and_creation_scope() {
        let mut m = model();
        m.selection.clear();
        m.selected = None;
        assert!(reason(&m, &Action::CopyFiles).is_some());
        assert!(reason(&m, &Action::NewEntry(true)).is_none());
        m.action(Action::NewEntry(true));
        assert!(m.rename_dialog.is_some());
        m.action(Action::RenameCancel);
        m.loaded = false;
        assert!(reason(&m, &Action::NewEntry(true)).is_some());
        m.loaded = true;
        m.mutations.busy = true;
        assert!(reason(&m, &Action::NewEntry(true)).is_some());
        m.mutations.busy = false;
        m.closing = true;
        assert!(reason(&m, &Action::NewEntry(true)).is_some());
    }
}
