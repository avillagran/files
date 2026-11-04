//! Fixture-only visible targets and shared navigation handlers; no OS input.
use crate::model::{Action, Model, View};
use std::path::Path;
fn click(m: &mut Model, wanted: impl Fn(&Action) -> bool, double: bool) -> Result<(), String> {
    let r = m
        .hits
        .iter()
        .rev()
        .find(|h| h.generation == m.generation && wanted(&h.action))
        .map(|h| h.rect)
        .ok_or("Target Espacio no visible")?;
    m.pointer_down(r.x + r.w / 2, r.y + r.h / 2, false, false, double);
    Ok(())
}
fn select(m: &mut Model, i: usize) -> Result<(), String> {
    let column = m.columns.columns.get(m.columns.active).map(|c| c.id);
    let pane = m.dual.active_id;
    let revision = m.pane_revision;
    click(
        m,
        |a| match a {
            Action::Entry(j) => *j == i,
            Action::PaneEntry(id, rev, j) => *id == pane && *rev == revision && *j == i,
            Action::ColumnEntry(id, j) => Some(*id) == column && *j == i,
            _ => false,
        },
        false,
    )
}
pub fn command(m: &mut Model, action: &str, root: &Path, w: i32, h: i32) -> Result<(), String> {
    if crate::directory_compare_ui::fixture_command(m, action)? {
        return Ok(());
    }
    if action == "fixture-copy-path-edit" {
        let mut copied = None;
        m.copy_path_with(|text| copied = Some(text.to_owned()));
        let text = copied.ok_or_else(|| m.error.clone())?;
        eprintln!(
            "FILES_PATH_TEXT {}",
            serde_json::json!({"text":text,"status":m.launch_notice})
        );
        m.begin_path_edit();
        m.path_input(&text).map_err(str::to_owned)?;
        return Ok(());
    }
    if action == "fixture-path-commit" {
        m.action(Action::PathCommit);
        return Ok(());
    }
    if let Some(input) = action.strip_prefix("key:") {
        use fltk::enums::Key;
        let (ctrl, input) = input
            .strip_prefix("ctrl-")
            .map_or((false, input), |s| (true, s));
        let (shift, input) = input
            .strip_prefix("shift-")
            .map_or((false, input), |s| (true, s));
        let key = match input {
            "home" => Key::Home,
            "end" => Key::End,
            "up" => Key::Up,
            "down" => Key::Down,
            "page-up" => Key::PageUp,
            "page-down" => Key::PageDown,
            _ => return Err("Tecla fixture desconocida".into()),
        };
        crate::keyboard_nav::key(m, key, ctrl, shift, w, h);
        return Ok(());
    }
    if let Some(raw) = action.strip_prefix("select-raw:") {
        if raw.len() > 8192 || !raw.len().is_multiple_of(2) {
            return Err("Nombre raw de fixture inválido".into());
        }
        let pairs = raw.as_bytes().as_chunks::<2>().0;
        let bytes = pairs
            .iter()
            .map(|pair| {
                std::str::from_utf8(pair)
                    .ok()
                    .and_then(|s| u8::from_str_radix(s, 16).ok())
                    .ok_or("Hex de fixture inválido")
            })
            .collect::<Result<Vec<_>, _>>()?;
        let i = m
            .entries
            .iter()
            .position(|e| e.name.as_encoded_bytes() == bytes)
            .ok_or("Entrada raw ausente")?;
        return select(m, i);
    }
    if let Some(criterion) = action.strip_prefix("header-sort:") {
        let sort = match criterion {
            "name" => files_core::listing::Sort::Name,
            "size" => files_core::listing::Sort::Size,
            "date" => files_core::listing::Sort::Modified,
            _ => return Err("Criterio de fixture inválido".into()),
        };
        let pane = m.dual.active_id;
        let revision = m.pane_revision;
        return click(
            m,
            |a| match a {
                Action::Sort(s) => *s == sort,
                Action::PaneSort(id, rev, s) => *id == pane && *rev == revision && *s == sort,
                _ => false,
            },
            false,
        );
    }
    if let Some(name) = action.strip_prefix("select:") {
        let i = m
            .entries
            .iter()
            .position(|e| e.name == name)
            .ok_or("Entrada fixture ausente")?;
        return select(m, i);
    }

    if let Some(index) = action.strip_prefix("bar:") {
        let index = index.parse::<usize>().map_err(|_| "Botón MC inválido")?;
        return click(
            m,
            |a| matches!(a, Action::OperationBar(_,_,i) if *i==index),
            false,
        );
    }
    if let Some(text) = action.strip_prefix("rename-text:") {
        m.rename_dialog
            .as_mut()
            .ok_or("Formulario de nombre ausente")?
            .input(text);
        return Ok(());
    }
    if let Some(name) = action.strip_prefix("copy-dest:") {
        if name.is_empty()
            || !std::path::Path::new(name)
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
        {
            return Err("Destino de fixture inválido".into());
        }
        m.copy_form
            .as_mut()
            .ok_or("Revisión de copia ausente")?
            .capture_destination(root.join(name));
        return Ok(());
    }
    if let Some(query) = action.strip_prefix("palette-open:") {
        m.open_commands();
        m.commands
            .as_mut()
            .ok_or("Paleta no disponible")?
            .input(query);
        return Ok(());
    }
    if let Some(id) = action.strip_prefix("palette:") {
        m.open_commands();
        let index = crate::command_palette::ITEMS
            .iter()
            .position(|s| s.id == id)
            .ok_or("Comando fixture desconocido")?;
        let state = m.commands.as_mut().ok_or("Paleta no disponible")?;
        state.matches = vec![(index, 0)];
        state.selected = 0;
        m.run_command();
        return Ok(());
    }
    if let Some(id) = action.strip_prefix("marks:") {
        let command = match id {
            "all" => crate::selection_tools::Command::All,
            "invert" => crate::selection_tools::Command::Invert,
            "clear" => crate::selection_tools::Command::Clear,
            _ => return Err("Selección fixture desconocida".into()),
        };
        m.action(Action::Selection(command));
        return Ok(());
    }
    match action {
        "state" => {}
        "pane-next" => m.switch_dual(),
        "fixture-rail:on" => {
            m.rail = true;
            m.hits.clear();
        }
        "fixture-rail:off" => {
            m.rail = false;
            m.hits.clear();
        }
        "copy-start-paused" => m.action(Action::CopyStartPaused),
        "copy-clear" => click(m, |a| matches!(a, Action::CopyClearPending), false)?,
        "copy-resume" => click(m, |a| matches!(a, Action::CopyPause(_, false)), false)?,
        "rename-apply" => click(m, |a| matches!(a, Action::RenameApply), false)?,
        "rename-close" => m.action(Action::RenameCancel),
        "properties-close" => m.action(Action::PropertyClose),
        "notice-close" => m.action(Action::DismissNotice),
        "copy-start" => click(m, |a| matches!(a, Action::CopyStart), false)?,
        "palette-close" => m.action(Action::CommandClose),
        "copy-inspector" => click(m, |a| matches!(a, Action::CopyFiles), false)?,
        "copy-close" => m.action(Action::CopyDialogCancel),
        "entry-menu" => {
            if !crate::context_menu::open_keyboard(m, w, h) {
                return Err("Menú de entrada no disponible".into());
            }
        }
        "menu-close" => crate::context_menu::key(m, fltk::enums::Key::Escape),
        "insert" => {
            m.mark_and_advance();
            crate::space_map_view::reveal(m);
        }
        "marks-menu" => click(m, |a| matches!(a, Action::SelectionMenu(_, _)), false)?,
        "menu-all" => {
            let i = m
                .context_menu
                .as_ref()
                .and_then(|menu| {
                    menu.items.iter().position(|i| {
                        matches!(
                            i.action,
                            Some(Action::Selection(crate::selection_tools::Command::All))
                        )
                    })
                })
                .ok_or("Marcar mapa no disponible")?;
            click(m, |a| matches!(a, Action::ContextItem(j) if *j == i), false)?;
        }
        "bar-toggle" => {
            crate::operation_bar::shortcut(m, fltk::enums::Key::from_char('b'), true, true);
        }
        "bar-recalculate" => click(m, |a| matches!(a, Action::OperationBar(_, _, 3)), false)?,
        "list-up" => click(m, |a| matches!(a,Action::SpaceListScroll(n) if *n<0), false)?,
        "list-down" => click(m, |a| matches!(a,Action::SpaceListScroll(n) if *n>0), false)?,
        "wheel-up" => crate::space_map_view::scroll(m, -1),
        "wheel-down" => crate::space_map_view::scroll(m, 1),
        "f5" => crate::keyboard_nav::f5(m),
        "hidden" => m.action(Action::ToggleHidden),
        "enter-selected" => {
            if !m
                .selected
                .and_then(|i| m.entries.get(i))
                .is_some_and(|e| e.is_dir())
            {
                return Err("Carpeta fixture no seleccionada".into());
            }
            m.open_selected();
        }
        "map" => click(m, |a| matches!(a, Action::View(View::SpaceMap)), false)?,
        "list" => click(m, |a| matches!(a, Action::View(View::List)), false)?,
        "full" => m.action(Action::WindowMode(
            files_core::theme::WindowMode::Fullscreen,
        )),
        "normal" => m.action(Action::WindowMode(files_core::theme::WindowMode::Normal)),
        "refresh" => click(m, |a| matches!(a, Action::SpaceRefresh), false)?,
        "cancel" => click(m, |a| matches!(a, Action::SpaceCancel), false)?,
        "back" => {
            let path = m.path.parent().ok_or("Sin carpeta padre")?.to_path_buf();
            click(m, |a| matches!(a,Action::Navigate(p) if *p==path), false)?;
        }
        "select-photos" | "enter-photos" | "select-zero" | "empty" => {
            let name = match action {
                "select-zero" => "vacío.txt",
                "empty" => "vacía",
                _ => "Fotos",
            };
            let i = m
                .entries
                .iter()
                .position(|e| e.name == name)
                .ok_or("Entrada fixture ausente")?;
            click(
                m,
                |a| matches!(a,Action::Entry(j) if *j==i),
                matches!(action, "enter-photos" | "empty"),
            )?;
        }
        "root" => m.action(Action::Navigate(root.to_path_buf())),
        "partial" | "busy" => m.action(Action::Navigate(
            root.parent().ok_or("Sin raíz fixture")?.join(action),
        )),
        "preview" => m.action(Action::Preview),
        "preview-close" => m.preview.close(),
        _ => return Err("Comando de fixture Espacio desconocido".into()),
    }
    Ok(())
}
pub fn marker(m: &Model, seq: u64) -> Option<String> {
    use crate::space_map_state::View as State;
    let state = match m.space_map.view() {
        State::Closed => "closed",
        State::Loading => "loading",
        State::Ready(_) => "ready",
        State::Error(_) => "error",
    };
    let stamp = if m.view == View::SpaceMap {
        match m.space_geometry.painted {
            Some(stamp) => stamp,
            None if m.preview.visible() || m.comparison.is_some() => {
                (m.space_map.revision(), m.generation, state, 0, 0)
            }
            None => return None,
        }
    } else {
        (m.space_map.revision(), m.generation, state, 0, 0)
    };
    if stamp.0 != m.space_map.revision() || stamp.1 != m.generation || stamp.2 != state {
        return None;
    }
    let report = match m.space_map.view() {
        State::Ready(r) => Some(r),
        _ => None,
    };
    let selected = m
        .selected
        .and_then(|i| m.entries.get(i))
        .map(|e| e.name.to_string_lossy());
    let areas=report.map(|report|m.space_geometry.areas.iter().take(32).map(|a|serde_json::json!({"name":String::from_utf8_lossy(&report.nodes[a.node].name),"bytes":report.nodes[a.node].bytes,"x":a.x,"y":a.y,"w":a.w,"h":a.h})).collect::<Vec<_>>()).unwrap_or_default();
    let hex = |i: usize| {
        m.entries.get(i).map(|e| {
            e.name
                .as_encoded_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        })
    };
    let list = m.space_geometry.list_rect;
    let queue = serde_json::json!({
        "copy_active":m.copy.active_id(),"copy_phase":format!("{:?}",m.copy.phase()),
        "copy_id":m.copy_id,"copy_queued_ids":m.copy.queued_ids().collect::<Vec<_>>(),
        "scene_ready":m.scene_ready(),"dual":m.dual.enabled,"pane_owner":m.pane_owner(),
        "sort":format!("{:?}",m.sort),"descending":m.descending,"other_sort":m.dual.other.as_ref().map(|b|format!("{:?}",b.sort)),
        "other_loaded":!m.dual.enabled || m.dual.other.as_ref().is_some_and(|b|b.loaded),
        "view_kind":match m.view {View::Groups=>"Groups",View::List=>"List",View::Grid=>"Grid",View::Columns=>"Columns",View::SpaceMap=>"SpaceMap",View::Spatial=>"Spatial"},
        "copy_controls":m.hits.iter().filter_map(|hit|match hit.action {
            Action::CopyPause(id,paused)=>Some(serde_json::json!({"action":if paused {"pause"}else{"resume"},"id":id,"rect":[hit.rect.x,hit.rect.y,hit.rect.w,hit.rect.h]})),
            Action::CopyCancel(id)=>Some(serde_json::json!({"action":"cancel","id":id,"rect":[hit.rect.x,hit.rect.y,hit.rect.w,hit.rect.h]})),
            Action::CopyDetails=>Some(serde_json::json!({"action":"details","rect":[hit.rect.x,hit.rect.y,hit.rect.w,hit.rect.h]})),
            Action::CopyClearPending=>Some(serde_json::json!({"action":"clear","rect":[hit.rect.x,hit.rect.y,hit.rect.w,hit.rect.h]})),_=>None}).collect::<Vec<_>>()});
    let extra = serde_json::json!({"comparison":crate::directory_compare_ui::fixture(m),"path_raw":m.path.as_os_str().as_encoded_bytes().iter().map(|b|format!("{b:02x}")).collect::<String>(),"mc":crate::operation_bar::fixture(m),"mutation_busy":m.mutations.busy,"mutation_applied":m.mutation_applied,"notice_open":m.notice_open,"can_undo":m.can_undo,"clipboard_count":m.clipboard_state.count(),"clipboard_busy":m.clipboard_state.busy(),"clipboard_kind":format!("{:?}",m.clipboard_state.kind()),"copy_busy":m.copy_busy(),"copy_queue":m.copy.queued_count(),"rename_dialog":m.rename_dialog.as_ref().map(|d|serde_json::json!({"mode":match d.mode {crate::rename_dialog::Mode::Rename=>"rename",crate::rename_dialog::Mode::File=>"file",crate::rename_dialog::Mode::Folder=>"folder"},"prepared":d.prepared.as_ref().is_some_and(|p|!p.is_empty()),"names_raw":d.selected.iter().map(|n|n.as_encoded_bytes().iter().map(|b|format!("{b:02x}")).collect::<String>()).collect::<Vec<_>>(),"error":d.error})),"properties":m.properties_dialog.as_ref().map(|d|serde_json::json!({"pending":d.pending,"observed":d.observed.is_some(),"path":d.path})),"preview_ready":matches!(m.preview.view(),crate::text_preview_state::View::Ready(_))});
    let mut value = serde_json::json!({"seq":seq,"view":if m.view==View::SpaceMap{"space_map"}else{"other"},"state":state,"changed":m.space_map.changed(),"watch_gap":m.space_map.watch_gap(),"watch_settled":m.listing_sync.settled(),"watch_pending":m.listing_sync.pending(),"revision":stamp.0,"generation":stamp.1,"width":stamp.3,"height":stamp.4,"path":crate::path_edit::display(&m.path).ok(),"loaded":m.loaded,"drained":m.space_map.is_drained(),"selected":selected,"selected_raw":m.selected.and_then(hex),"palette":crate::command_palette::fixture_state(m),"copy_dialog":m.copy_form.as_ref().map(|f|serde_json::json!({"moving":f.moving,"reviewed":f.prepared.is_some(),"creating":f.creating,"extracting":f.extracting,"error":f.error,"names_raw":f.names.iter().map(|n|n.as_encoded_bytes().iter().map(|b|format!("{b:02x}")).collect::<String>()).collect::<Vec<_>>()})),"selection_error":m.error,"selection_menu":m.context_menu.as_ref().map(|menu|menu.items.iter().map(|i|serde_json::json!({"label":i.label,"enabled":i.action.is_some()})).collect::<Vec<_>>()),"selection_raw":m.selection.iter().filter_map(|&i|hex(i)).collect::<Vec<_>>(),"navigation_raw":m.space_geometry.navigation_indices().iter().filter_map(|&i|hex(i)).collect::<Vec<_>>(),"no_area_raw":m.space_geometry.excluded_entries().filter_map(hex).collect::<Vec<_>>(),"row_hits":m.hits.iter().filter_map(|hit|if let Action::Entry(i)=hit.action { if hit.rect.y>=list.y && hit.rect.y+hit.rect.h<=list.y+list.h { Some(serde_json::json!({"raw":hex(i),"rect":[hit.rect.x,hit.rect.y,hit.rect.w,hit.rect.h]})) }else{None} }else{None}).collect::<Vec<_>>(),"list_rect":[list.x,list.y,list.w,list.h],"scroll":m.scroll,"max_scroll":m.space_geometry.max_scroll(),"page_rows":m.space_geometry.page_rows(),"operation_bar":m.operation_bar,"bar_rects":m.hits.iter().filter_map(|hit|if let Action::OperationBar(_,_,i)=hit.action { Some(serde_json::json!({"index":i,"rect":[hit.rect.x,hit.rect.y,hit.rect.w,hit.rect.h]})) }else{None}).collect::<Vec<_>>(),"total":report.map(|r|r.total_bytes),"observed":report.map(|r|r.observed_bytes),"complete":report.map(|r|r.complete),"nodes":report.map(|r|r.nodes.len()),"areas":areas,"controls":m.hits.iter().filter_map(|hit|match hit.action { Action::SpaceRefresh=>Some(serde_json::json!({"action":"recalculate","rect":[hit.rect.x,hit.rect.y,hit.rect.w,hit.rect.h]})),Action::SpaceCancel=>Some(serde_json::json!({"action":"cancel","rect":[hit.rect.x,hit.rect.y,hit.rect.w,hit.rect.h]})),Action::SpaceListScroll(n)=>Some(serde_json::json!({"action":if n<0 {"list-up"}else{"list-down"},"rect":[hit.rect.x,hit.rect.y,hit.rect.w,hit.rect.h]})),_=>None }).collect::<Vec<_>>(),"error":match m.space_map.view(){State::Error(e)=>Some(e.as_str()),_=>None},"preview_visible":m.preview.visible(),"preview_drained":m.preview.is_drained()});
    if let serde_json::Value::Object(queue) = queue {
        value.as_object_mut().unwrap().extend(queue);
    }
    if let serde_json::Value::Object(extra) = extra {
        value.as_object_mut().unwrap().extend(extra);
    }
    Some(format!("FILES_SPACE_STATE {value}"))
}
