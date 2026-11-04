//! Optional MC-style controls. No captured file payload, I/O or operation engine.
#[cfg(test)]
use crate::model::View;
use crate::{
    model::{Action, Hit, Model},
    painter::{Painter, Rect},
};
use fltk::enums::Key;

pub const HEIGHT: i32 = 36;
const ITEMS: [(&str, &str, &str, &str); 8] = [
    ("F1", "", "Ayuda", "Ayuda y atajos, F1"),
    ("F2", "󰑕", "Renombrar", "Renombrar selección, F2"),
    ("F3", "", "Previa", "Previsualizar elemento enfocado, F3"),
    ("F5", "", "Copiar", "Copiar selección: revisar destino, F5"),
    ("F6", "", "Mover", "Mover selección: revisar destino, F6"),
    ("F7", "", "Carpeta", "Crear carpeta, F7"),
    ("F8", "", "Papelera", "Enviar selección a Papelera, F8"),
    ("F10", "", "Acciones", "Acciones sobre archivos, F10"),
];
pub fn label(index: usize) -> Option<&'static str> {
    if index == 3 {
        return Some("F5: copiar; en Espacio, recalcular");
    }
    ITEMS.get(index).map(|v| v.3)
}
fn action_for(m: &Model, index: usize) -> Option<Action> {
    if index == 3 {
        Some(crate::keyboard_nav::f5_action(m))
    } else {
        action(index)
    }
}
pub fn reserve(m: &Model) -> i32 {
    if m.operation_bar { HEIGHT } else { 0 }
}
pub fn browser_height(m: &Model, h: i32) -> i32 {
    h - reserve(m)
        - if crate::mutations::compact_notice_visible(m) {
            48
        } else {
            0
        }
}
pub fn action(index: usize) -> Option<Action> {
    Some(match index {
        0 => Action::Help,
        1 => Action::Rename,
        2 => Action::Preview,
        3 => Action::CopyFiles,
        4 => Action::MoveFiles,
        5 => Action::NewEntry(true),
        6 => Action::Trash,
        7 => Action::Actions,
        _ => return None,
    })
}
fn modal(m: &Model) -> bool {
    m.closing
        || m.bookmarks.panel.is_some()
        || m.preview.visible()
        || m.search.visible
        || m.commands.is_some()
        || m.size_panel.is_some()
        || m.sftp.form.is_some()
        || m.help.is_some()
        || m.rename_dialog.is_some()
        || m.copy_form.is_some()
        || m.properties_dialog.is_some()
        || m.notice_open
        || m.recovery_catalog.open
        || m.mutation_applied
}
pub fn reason(m: &Model, index: usize) -> Option<&'static str> {
    action(index)?;
    if modal(m) {
        return Some("Cierra el diálogo o herramienta primero");
    }
    if m.editing || m.filter_edit.active {
        return Some("Enter: vuelve a la lista");
    }
    if index == 0 || index == 7 {
        return None;
    }
    if !m.loaded || m.selection_refreshing() {
        return Some("Espera la carga");
    }
    if crate::selection_tools::is_map(m) {
        return if index == 3 {
            None
        } else {
            crate::map_actions::reason(m, &action_for(m, index)?)
        };
    }
    let action = action_for(m, index)?;
    if crate::copy_review::supported(&action) {
        return crate::copy_review::reason(m, &action);
    }
    if index != 2 && (m.mutations.busy || m.copy_busy()) {
        return Some("Operación en curso");
    }
    if index == 2 {
        if m.selected.and_then(|i| m.entries.get(i)).is_none() {
            return Some("Enfoca un elemento primero");
        }
    } else if index != 5 && m.selection.is_empty() && m.selected.is_none() {
        return Some("Selecciona al menos un elemento");
    }
    None
}
impl Model {
    pub fn toggle_operation_bar(&mut self) {
        if modal(self) {
            return;
        }
        if self.error.starts_with("MC: ") {
            self.error.clear();
        }
        self.operation_bar = !self.operation_bar;
        self.hits.clear();
        self.visible.clear();
        self.search_reveal = true;
        self.config_revision += 1;
        self.config_dirty = true;
        self.config_error = None;
        self.dispatch_config();
    }
    pub fn operation_bar_run(&mut self, owner: (u64, u64), generation: u64, index: usize) {
        // A click deliberately uses the current selection, just like its F-key.
        // Never use a previously painted pane/root after navigation or refresh.
        if !self.operation_bar
            || owner != self.pane_owner()
            || generation != self.generation
            || action(index).is_none()
        {
            return;
        }
        if let Some(why) = reason(self, index) {
            if !modal(self) {
                self.error = format!("MC: {why}");
            }
            return;
        }
        if self.error.starts_with("MC: ") {
            self.error.clear();
        }
        self.action(action_for(self, index).unwrap());
    }
}
pub fn shortcut(m: &mut Model, key: Key, ctrl: bool, alt: bool) -> bool {
    if ctrl && alt && key == Key::from_char('b') {
        m.toggle_operation_bar();
        true
    } else {
        false
    }
}
pub fn pointer(m: &mut Model, x: i32, y: i32) -> bool {
    let hit = m
        .hits
        .iter()
        .rev()
        .find(|h| {
            h.generation == m.generation
                && h.rect.contains(x, y)
                && matches!(h.action, Action::OperationBar(..))
        })
        .map(|h| h.action.clone());
    if let Some(action) = hit {
        m.action(action);
        true
    } else {
        false
    }
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, w: i32, h: i32) {
    if !m.operation_bar {
        return;
    }
    let y = h - 32 - HEIGHT;
    let c = m.palette.clone();
    p.rect(Rect::new(0, y, w, HEIGHT), c.darker_background);
    p.rect(Rect::new(0, y, w, 1), c.lighter_background);
    for (i, (key, icon, text, _)) in ITEMS.iter().enumerate() {
        let x = i as i32 * w / 8;
        let end = (i as i32 + 1) * w / 8;
        let area = Rect::new(x + 2, y + 3, end - x - 4, 30);
        let enabled = reason(m, i).is_none();
        p.rounded_rect(
            area,
            4 * m.radius,
            if enabled {
                c.lighter_background
            } else {
                c.darker_background
            },
        );
        let (icon, text) = if i == 3 && crate::selection_tools::is_map(m) {
            ("", "Recalcular")
        } else {
            (*icon, *text)
        };
        let value = if w >= 960 {
            format!("{key} {icon} {text}")
        } else if w >= 720 {
            format!("{key} {text}")
        } else {
            format!("{key} {icon}")
        };
        p.text(
            Rect::new(area.x + 3, area.y, area.w - 6, area.h),
            &value,
            11,
            if enabled { c.foreground } else { c.muted },
            false,
            false,
        );
        // Disabled buttons may explain why, but cannot launch an operation.
        if !modal(m) {
            m.hits.push(Hit {
                rect: area,
                generation: m.generation,
                action: Action::OperationBar(m.pane_owner(), m.generation, i),
            });
        }
    }
}
pub fn fixture(m: &Model) -> serde_json::Value {
    serde_json::json!({"enabled":m.operation_bar,"buttons":m.hits.iter().filter_map(|h|if let Action::OperationBar(owner,generation,index)=h.action {Some(serde_json::json!({"index":index,"owner":owner,"generation":generation,"label":label(index),"reason":reason(m,index),"rect":[h.rect.x,h.rect.y,h.rect.w,h.rect.h]}))}else{None}).collect::<Vec<_>>()})
}

#[cfg(test)]
mod tests {
    use super::*;
    use files_core::{
        listing::{Entry, Kind},
        theme::Color,
    };
    fn model() -> Model {
        let mut m = Model::new("/fixture-only-operation-bar".into());
        // Este mock de AuxIo prueba otro subsistema; el catálogo inicial ya está cargado.
        m.folder_views = Default::default();
        m.folder_views.loaded = true;
        m.folder_restore = None;
        m.operation_bar = true;
        m.loaded = true;
        m.view = View::List;
        m.entries = (0..100)
            .map(|i| Entry {
                name: format!("item-{i:03}").into(),
                kind: Kind::File,
                metadata: None,
                stat_attempted: false,
            })
            .collect();
        m.rebuild();
        m.selected = Some(0);
        m.selection.insert(0);
        m
    }
    #[derive(Default)]
    struct Recorder;
    impl Painter for Recorder {
        fn rect(&mut self, _: Rect, _: Color) {}
        fn rounded_rect(&mut self, _: Rect, _: i32, _: Color) {}
        fn text(&mut self, _: Rect, _: &str, _: i32, _: Color, _: bool, _: bool) {}
        fn image(&mut self, _: Rect, _: &crate::painter::Image<'_>) {}
        fn clip(&mut self, _: Rect) {}
        fn unclip(&mut self) {}
    }
    #[test]
    fn stale_pane_generation_disabled_and_editor_click_never_launch_or_blur() {
        let mut m = model();
        let owner = m.pane_owner();
        let generation = m.generation;
        m.operation_bar_run((owner.0 + 1, owner.1), generation, 3);
        m.operation_bar_run(owner, generation + 1, 3);
        assert!(m.copy_form.is_none());
        assert!(m.error.is_empty());
        m.operation_bar_run(owner, generation, 99);
        assert!(m.error.is_empty());
        m.action(Action::Filter);
        assert!(m.filter_edit.active);
        paint(&mut Recorder, &mut m, 800, 440);
        let r = m
            .hits
            .iter()
            .find(|h| matches!(h.action, Action::OperationBar(_, _, 3)))
            .unwrap()
            .rect;
        assert!(pointer(&mut m, r.x + 2, r.y + 2));
        assert!(m.filter_edit.active);
        assert!(m.copy_form.is_none());
        assert!(m.error.contains("Enter"));
        m.end_filter_edit();
        m.selection.clear();
        m.selected = None;
        m.operation_bar_run(owner, generation, 3);
        assert!(m.copy_form.is_none());
        assert!(m.error.contains("Selecciona"));
        m.mutations.busy = true;
        assert!(reason(&m, 1).unwrap().contains("curso"));
        m.mutations.busy = false;
        m.view = View::SpaceMap;
        assert!(reason(&m, 3).is_none());
        assert!(matches!(action_for(&m, 3), Some(Action::SpaceRefresh)));
        assert!(reason(&m, 4).unwrap().contains("Selecciona"));
    }
    #[test]
    fn dual_listing_over_retained_map_uses_listing_commands() {
        let mut m = model();
        m.view = View::SpaceMap;
        m.dual.enabled = true;
        assert!(reason(&m, 3).is_none());
        assert!(reason(&m, 4).is_none());
        assert!(matches!(action_for(&m, 3), Some(Action::CopyFiles)));
        assert!(matches!(
            crate::keyboard_nav::f5_action(&m),
            Action::CopyFiles
        ));
    }
    #[test]
    fn reserved_geometry_reveals_end_and_keeps_every_view_and_hit_above_bar() {
        let mut m = model();
        for (w, h) in [(440, 440), (800, 440), (1000, 650)] {
            for view in [View::List, View::Groups, View::Grid, View::Columns] {
                m.view = view;
                m.scroll = 0;
                m.selected = m.order.last().copied();
                crate::view::reveal_selection(&mut m, w, h);
                crate::view::paint(&mut Recorder, &mut m, w, h);
                if matches!(view, View::List | View::Grid) {
                    assert!(m.visible.contains(&99), "focused end row not visible");
                }
                let y = h - 32 - HEIGHT;
                let buttons: Vec<_> = m
                    .hits
                    .iter()
                    .filter(|v| matches!(v.action, Action::OperationBar(..)))
                    .collect();
                assert_eq!(buttons.len(), 8);
                for b in &buttons {
                    assert!(
                        b.rect.x >= 0
                            && b.rect.x + b.rect.w <= w
                            && b.rect.y >= y
                            && b.rect.y + b.rect.h <= h - 32
                    );
                }
                for row in m.hits.iter().filter(|v| {
                    matches!(
                        v.action,
                        Action::Entry(_) | Action::ColumnEntry(..) | Action::PaneEntry(..)
                    )
                }) {
                    assert!(row.rect.y + row.rect.h <= y, "row overlaps strip");
                }
            }
        }
        m.view = View::List;
        m.dual.enabled = true;
        m.scroll = 0;
        m.selected = Some(99);
        crate::view::reveal_selection(&mut m, 800, 440);
        crate::view::paint(&mut Recorder, &mut m, 800, 440);
        assert!(m.visible.contains(&99));
        for hit in m
            .hits
            .iter()
            .filter(|h| matches!(h.action, Action::PaneEntry(..)))
        {
            assert!(hit.rect.y + hit.rect.h <= 372);
        }
        assert_eq!(browser_height(&m, 440), 404);
        m.operation_bar = false;
        assert_eq!(browser_height(&m, 440), 440);
    }
    #[test]
    fn preference_snapshot_coalesces_and_preserves_final_bar_value() {
        use crate::aux_io::{AuxIo, Request, Response};
        use std::sync::mpsc;
        let mut m = model();
        let (tx, input) = mpsc::sync_channel(1);
        let (output, rx) = mpsc::channel();
        m.aux = AuxIo { tx, rx };
        m.toggle_operation_bar();
        let Request::Preferences(rev, _, _, _, bar, _) = input.try_recv().unwrap() else {
            panic!()
        };
        assert!(!bar);
        m.toggle_operation_bar();
        assert!(m.config_dirty);
        output.send(Response::Config(rev, Ok(()))).unwrap();
        m.pump();
        let Request::Preferences(next, _, _, _, bar, _) = input.try_recv().unwrap() else {
            panic!()
        };
        assert!(bar && next > rev);
        assert!(m.config_in_flight.is_some());
    }
    #[test]
    fn map_f5_and_mc_button_recalculate_while_list_f5_keeps_copy_review() {
        let mut m = model();
        m.operation_bar = true;
        m.view = View::SpaceMap;
        let before = m.generation;
        crate::keyboard_nav::f5(&mut m);
        assert_ne!(m.generation, before);
        assert!(m.space_requested && m.copy_form.is_none());
        m.loaded = true;
        m.refresh_selection = None;
        let owner = m.pane_owner();
        let generation = m.generation;
        m.operation_bar_run(owner, generation, 3);
        assert_ne!(m.generation, generation);
        assert!(m.space_requested && m.copy_form.is_none());
        m.view = View::List;
        assert!(matches!(action_for(&m, 3), Some(Action::CopyFiles)));
        let mut list = model();
        let generation = list.generation;
        crate::keyboard_nav::f5(&mut list);
        assert_eq!(list.generation, generation);
        assert!(list.copy_form.is_some());
    }
}

#[cfg(test)]
mod compact_notice_tests {
    use super::*;
    use files_core::{
        listing::{Entry, Kind},
        theme::Color,
    };
    #[derive(Default)]
    struct Texts(Vec<(String, Rect)>);
    impl Painter for Texts {
        fn rect(&mut self, _: Rect, _: Color) {}
        fn rounded_rect(&mut self, _: Rect, _: i32, _: Color) {}
        fn text(&mut self, r: Rect, t: &str, _: i32, _: Color, _: bool, _: bool) {
            self.0.push((t.into(), r));
        }
        fn image(&mut self, _: Rect, _: &crate::painter::Image<'_>) {}
        fn clip(&mut self, _: Rect) {}
        fn unclip(&mut self) {}
    }
    #[test]
    fn compact_notice_and_mc_strip_do_not_cover_dual_counters_or_file_targets() {
        let mut m = Model::new("/fixture-notice-layout".into());
        m.loaded = true;
        m.view = View::List;
        m.dual.enabled = true;
        m.entries = vec![Entry {
            name: "one".into(),
            kind: Kind::File,
            metadata: None,
            stat_attempted: true,
        }];
        m.rebuild();
        m.operation_message = Some("NOTICE_LAYOUT".into());
        for (w, h) in [(440, 440), (800, 500), (1000, 650)] {
            for bar in [false, true] {
                m.operation_bar = bar;
                let mut p = Texts::default();
                crate::view::paint(&mut p, &mut m, w, h);
                let notice = p.0.iter().find(|(t, _)| t == "NOTICE_LAYOUT").unwrap().1;
                let footer =
                    p.0.iter()
                        .find(|(t, _)| t.contains("visibles / ") || t.ends_with("marc."))
                        .unwrap()
                        .1;
                assert!(footer.y + footer.h <= notice.y);
                for target in &m.hits {
                    match target.action {
                        Action::PaneEntry(..) => assert!(target.rect.y + target.rect.h <= footer.y),
                        Action::OperationBar(..) => assert!(notice.y + notice.h <= target.rect.y),
                        _ => {}
                    }
                }
            }
        }
        m.notice_open = true;
        assert!(!crate::mutations::compact_notice_visible(&m));
        assert_eq!(browser_height(&m, 440), 440 - HEIGHT);
        m.notice_open = false;
        m.closing = true;
        assert!(!crate::mutations::compact_notice_visible(&m));
    }
}
