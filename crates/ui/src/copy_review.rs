//! Revisión de transferencias: no admite trabajos ni cambia la cola.
use crate::model::{Action, Model};
pub fn supported(action: &Action) -> bool {
    matches!(
        action,
        Action::CopyFiles | Action::MoveFiles | Action::CreateArchive | Action::ExtractArchive
    )
}
pub fn pending_reason(m: &Model) -> Option<&'static str> {
    if m.closing {
        Some("El explorador se está cerrando")
    } else if m.move_preparing.is_some() || m.archive_preparing.is_some() {
        Some("Espera la revisión cancelada antes de abrir otro destino")
    } else if m.mutation_applied {
        Some("Espera la operación en curso")
    } else {
        None
    }
}
/// Consulta de pintura: cuenta marcas sin recorrerlas ni consultar el FS.
pub fn reason(m: &Model, action: &Action) -> Option<&'static str> {
    if !supported(action) {
        return None;
    }
    if let Some(why) = pending_reason(m) {
        return Some(why);
    }
    if !m.loaded || m.selection_refreshing() {
        return Some("Espera la carga de la carpeta");
    }
    if m.editing || m.filter_edit.active {
        return Some("Enter: vuelve al explorador");
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
    let count = if m.selection.is_empty() {
        usize::from(
            m.selected
                .and_then(|i| m.entries.get(i))
                .is_some_and(|e| files_core::visibility::accepts(&e.name, m.show_hidden)),
        )
    } else {
        m.selection.len()
    };
    if count == 0 {
        return Some("Selecciona al menos un elemento");
    }
    match action {
        Action::MoveFiles if count > files_core::move_engine::MAX_ROOTS => {
            Some("Selecciona hasta 128 raíces para mover")
        }
        Action::CreateArchive if count > 128 => Some("Selecciona hasta 128 raíces"),
        Action::ExtractArchive if count != 1 => Some("Selecciona un ZIP para extraer"),
        _ => None,
    }
}
/// Comprueba la selección una vez, antes de sanear o capturar otro foco.
pub fn validate(m: &Model, action: &Action) -> Option<&'static str> {
    if !supported(action) {
        return None;
    }
    if let Some(why) = reason(m, action) {
        return Some(why);
    }
    let filter = (!crate::selection_tools::is_map(m) && !m.query.is_empty())
        .then(|| files_core::name_filter::Filter::new(&m.query));
    let valid = |i| {
        m.entries
            .get(i)
            .is_some_and(|e: &files_core::listing::Entry| {
                files_core::visibility::accepts(&e.name, m.show_hidden)
                    && filter.as_ref().is_none_or(|f| f.score(e).is_some())
            })
    };
    let invalid = if m.selection.is_empty() {
        m.selected.is_none_or(|i| !valid(i))
    } else {
        m.selection.iter().any(|&i| !valid(i))
    };
    invalid.then_some("La selección cambió; vuelve a marcar los elementos")
}

#[cfg(test)]
mod tests {
    use super::*;
    use files_core::listing::{Entry, Kind};
    fn model() -> Model {
        let mut m = Model::new("/fixture-copy-review".into());
        m.loaded = true;
        m.view = crate::model::View::List;
        m.operation_bar = true;
        m.entries = ["one.txt", "two.txt", ".hidden"]
            .into_iter()
            .map(|name| Entry {
                name: name.into(),
                kind: Kind::File,
                metadata: None,
                stat_attempted: true,
            })
            .collect();
        m.order = vec![0, 1];
        m.selection.extend([0, 1]);
        m.selected = Some(0);
        m
    }
    fn queue(m: &mut Model, n: u64) {
        for id in 1..=n {
            m.copy
                .enqueue(
                    crate::copy_job::Request::new(
                        id,
                        vec![files_core::copy_engine::CopyRequest {
                            source: format!("/fixture-copy-review/input-{id}").into(),
                            destination: format!("/fixture-copy-review-out/output-{id}").into(),
                        }],
                        Default::default(),
                    )
                    .start_paused(),
                )
                .unwrap();
        }
        m.copy_id = n;
    }
    struct Recorder;
    impl crate::painter::Painter for Recorder {
        fn rect(&mut self, _: crate::painter::Rect, _: files_core::theme::Color) {}
        fn rounded_rect(&mut self, _: crate::painter::Rect, _: i32, _: files_core::theme::Color) {}
        fn text(
            &mut self,
            _: crate::painter::Rect,
            _: &str,
            _: i32,
            _: files_core::theme::Color,
            _: bool,
            _: bool,
        ) {
        }
        fn image(&mut self, _: crate::painter::Rect, _: &crate::painter::Image<'_>) {}
        fn clip(&mut self, _: crate::painter::Rect) {}
        fn unclip(&mut self) {}
    }
    #[test]
    fn active_progress_controls_stay_inside_viewport_and_above_mc_with_expanded_rail() {
        let mut m = model();
        queue(&mut m, 1);
        m.copy.pump();
        for (w, h) in [(440, 440), (800, 440), (1000, 650)] {
            for rail in [false, true] {
                for dual in [false, true] {
                    m.rail = rail;
                    m.dual.enabled = dual;
                    m.hits.clear();
                    crate::copy_ui::paint(&mut Recorder, &mut m, w, h);
                    let card = crate::copy_ui::progress_layout(&m, w, h).rect;
                    assert!(
                        card.x >= 0
                            && card.x + card.w <= w
                            && card.y >= 90
                            && card.y + card.h <= h - 68
                    );
                    let controls = m
                        .hits
                        .iter()
                        .filter(|h| {
                            matches!(
                                h.action,
                                Action::CopyPause(..)
                                    | Action::CopyCancel(_)
                                    | Action::CopyClearPending
                            )
                        })
                        .map(|h| h.rect)
                        .collect::<Vec<_>>();
                    assert_eq!(controls.len(), 3);
                    for (i, a) in controls.iter().enumerate() {
                        assert!(
                            a.x >= card.x
                                && a.y >= card.y
                                && a.x + a.w <= card.x + card.w
                                && a.y + a.h <= card.y + card.h
                        );
                        for b in &controls[i + 1..] {
                            assert!(
                                a.x + a.w <= b.x
                                    || b.x + b.w <= a.x
                                    || a.y + a.h <= b.y
                                    || b.y + b.h <= a.y
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn mc_and_palette_review_while_queued_preserve_names_and_jobs_in_every_profile() {
        use crate::model::View;
        for view in [
            View::List,
            View::Groups,
            View::Grid,
            View::Columns,
            View::SpaceMap,
        ] {
            let mut m = model();
            m.view = view;
            #[cfg(unix)]
            {
                use std::os::unix::ffi::OsStringExt;
                m.entries[1].name = std::ffi::OsString::from_vec(b"raw\xff\n.txt".to_vec());
            }
            let expected = m.entries[..2]
                .iter()
                .map(|e| e.name.clone())
                .collect::<Vec<_>>();
            queue(&mut m, 1);
            let owner = m.pane_owner();
            let generation = m.generation;
            // En Espacio F5 recalcula; el botón Mover sí abre una revisión.
            m.operation_bar_run(
                owner,
                generation,
                if view == View::SpaceMap { 4 } else { 3 },
            );
            assert_eq!(
                m.copy_form.as_ref().unwrap().names.as_ref(),
                expected.as_slice()
            );
            m.action(Action::CopyDialogCancel);
            m.open_commands();
            let i = crate::command_palette::ITEMS
                .iter()
                .position(|s| s.id == "move")
                .unwrap();
            let s = m.commands.as_mut().unwrap();
            s.matches = vec![(i, 0)];
            s.selected = 0;
            m.run_command();
            let form = m.copy_form.as_ref().unwrap();
            assert!(form.moving);
            assert_eq!(form.names.as_ref(), expected.as_slice());
            assert_eq!(m.copy.queued_ids().collect::<Vec<_>>(), vec![1]);
        }
    }
    #[test]
    fn full_queue_allows_review_and_rejection_retains_form_without_admission() {
        let mut m = model();
        queue(&mut m, 8);
        m.action(Action::CopyFiles);
        m.copy_form
            .as_mut()
            .unwrap()
            .capture_destination("/fixture-copy-review-out".into());
        m.action(Action::CopyStart);
        let form = m.copy_form.as_ref().unwrap();
        assert!(form.error.contains("ocho"), "{}", form.error);
        assert_eq!(form.names.len(), 2);
        assert_eq!(m.copy.queued_count(), 8);
        assert!(m.copy.active_id().is_none());
    }
    #[test]
    fn pending_review_ack_applied_mutation_and_closing_still_block_another_form() {
        let mut m = model();
        queue(&mut m, 1);
        m.move_preparing = Some((
            9,
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
        ));
        m.action(Action::CopyFiles);
        assert!(m.copy_form.is_none());
        assert!(m.error.contains("revisión cancelada"));
        m.move_preparing = None;
        m.mutation_applied = true;
        m.action(Action::CopyFiles);
        assert!(m.copy_form.is_none());
        m.mutation_applied = false;
        m.closing = true;
        m.action(Action::CopyFiles);
        assert!(m.copy_form.is_none());
        assert_eq!(m.copy.queued_count(), 1);
    }
    #[test]
    fn filtered_hidden_or_stale_marks_never_turn_into_a_different_focus_capture() {
        let mut m = model();
        queue(&mut m, 1);
        for (index, query) in [(1, "one"), (2, ""), (999, "")] {
            m.selection.clear();
            m.selection.insert(index);
            m.selected = Some(0);
            m.query = query.into();
            m.action(Action::CopyFiles);
            assert!(m.copy_form.is_none());
            assert!(m.error.contains("selección cambió"));
            assert_eq!(m.selection.iter().copied().collect::<Vec<_>>(), vec![index]);
            assert_eq!(m.selected, Some(0));
        }
        m.query.clear();
        m.selection.clear();
        m.selected = None;
        m.action(Action::CopyFiles);
        assert!(m.copy_form.is_none());
        m.selected = Some(0);
        m.loaded = false;
        m.action(Action::CopyFiles);
        assert!(m.copy_form.is_none());
        m.loaded = true;
        m.action(Action::CopyFiles);
        assert!(m.copy_form.is_some());
        assert!(m.error.is_empty());
    }
}
