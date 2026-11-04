//! Extension of the owned tabs fixture. Shared handlers, never native input.
use crate::{
    model::{Action, Model},
    painter::Rect,
};
fn click(
    m: &mut Model,
    predicate: impl Fn(&Action) -> bool,
    toggle: bool,
    double: bool,
) -> Result<(), String> {
    let r = m
        .hits
        .iter()
        .rev()
        .find(|h| h.generation == m.generation && predicate(&h.action))
        .map(|h| h.rect)
        .ok_or("Control de panel no visible")?;
    m.pointer_down(r.x + r.w / 2, r.y + r.h / 2, toggle, false, double);
    Ok(())
}
pub fn command(m: &mut Model, action: &str, w: i32, h: i32) -> Result<(), String> {
    if let Some(query) = action.strip_prefix("dual-filter:") {
        crate::input(m, query);
        return Ok(());
    }
    if let Some(name) = action.strip_prefix("dual-select:") {
        let i = m
            .entries
            .iter()
            .position(|e| e.name == name)
            .ok_or("Entrada fixture ausente")?;
        let id = m.dual.active_id;
        return click(
            m,
            |a| matches!(a,Action::PaneEntry(p,_,j)if *p==id&&*j==i),
            false,
            false,
        );
    }
    if let Some(spec) = action.strip_prefix("dual-sort:") {
        let (pane, criterion) = spec.split_once(':').ok_or("Orden fixture inválido")?;
        let id = if pane == "active" {
            m.dual.active_id
        } else if pane == "other" {
            m.dual.other.as_ref().ok_or("Sin otro panel")?.id
        } else {
            return Err("Panel fixture inválido".into());
        };
        let sort = match criterion {
            "name" => files_core::listing::Sort::Name,
            "size" => files_core::listing::Sort::Size,
            "date" => files_core::listing::Sort::Modified,
            _ => return Err("Criterio fixture inválido".into()),
        };
        return click(
            m,
            |a| matches!(a,Action::PaneSort(p,_,s)if *p==id&&*s==sort),
            false,
            false,
        );
    }
    if let Some(spec) = action.strip_prefix("dual-filter-control:") {
        let (pane, mode) = spec.split_once(':').ok_or("Filtro fixture inválido")?;
        let id = if pane == "active" {
            m.dual.active_id
        } else if pane == "other" {
            m.dual.other.as_ref().ok_or("Sin otro panel")?.id
        } else {
            return Err("Panel inválido".into());
        };
        let clear = match mode {
            "clear" => true,
            "edit" => false,
            _ => return Err("Modo inválido".into()),
        };
        return click(
            m,
            |a| matches!(a,Action::PaneFilter(p,_,c)if *p==id&&*c==clear),
            false,
            false,
        );
    }
    match action {
        "dual-on" => m.action(Action::DualPane),
        "dual-switch" => m.switch_dual(),
        "dual-other-header" => {
            let id = m.dual.other.as_ref().ok_or("Sin panel secundario")?.id;
            click(
                m,
                |a| matches!(a,Action::PaneFocus(i)if *i==id),
                false,
                false,
            )?;
        }
        "dual-b-location" => m.navigate(
            std::env::var_os("FILES_TEST_DUAL_B")
                .ok_or("Destino privado ausente")?
                .into(),
        ),
        "dual-folder" => {
            let id = m.dual.active_id;
            let i = m
                .entries
                .iter()
                .position(|e| e.name == "folder")
                .ok_or("Sin folder")?;
            click(
                m,
                |a| matches!(a,Action::PaneEntry(p,_,j)if *p==id&&*j==i),
                false,
                true,
            )?;
        }
        "dual-back" => m.travel_history(false),
        "dual-filter" => crate::input(m, "same"),
        "dual-clear-filter" => {
            m.query.clear();
            m.rebuild();
        }
        "dual-batch" => {
            for (name, toggle) in [("same.txt", false), ("other.txt", true)] {
                let i = m
                    .entries
                    .iter()
                    .position(|e| e.name == name)
                    .ok_or("Lote ausente")?;
                let id = m.dual.active_id;
                click(
                    m,
                    |a| matches!(a,Action::PaneEntry(p,_,j)if *p==id&&*j==i),
                    toggle,
                    false,
                )?;
            }
        }
        "dual-menu-other" => {
            let b = m.dual.other.as_ref().ok_or("Sin otro panel")?;
            let i = b
                .entries
                .iter()
                .position(|e| e.name == "same.txt")
                .ok_or("Sin same")?;
            let id = b.id;
            let r = m
                .hits
                .iter()
                .find(|h| {
                    h.generation == m.generation
                        && matches!(h.action,Action::PaneEntry(p,_,j)if p==id&&j==i)
                })
                .ok_or("Entrada no visible")?
                .rect;
            if !crate::context_menu::open_at(m, r.x + r.w / 2, r.y + r.h / 2, w, h) {
                return Err("Menú rechazado".into());
            }
        }
        "dual-menu-key" => {
            if !crate::context_menu::open_keyboard(m, w, h) {
                return Err("Menú teclado rechazado".into());
            }
        }
        "dual-menu-close" => crate::context_menu::dismiss(m),
        "dual-copy" => m.action(Action::CopyFiles),
        "dual-move" => m.action(Action::MoveFiles),
        "dual-copy-raw" | "dual-move-raw" => {
            let prefix = if action == "dual-copy-raw" {
                b"copy-"
            } else {
                b"move-"
            };
            let i = m
                .entries
                .iter()
                .position(|e| e.name.as_encoded_bytes().starts_with(prefix))
                .ok_or("Raw fixture ausente")?;
            m.select_entry(i, false, false);
            m.action(if action == "dual-copy-raw" {
                Action::CopyFiles
            } else {
                Action::MoveFiles
            });
        }
        "dual-paused" => click(m, |a| matches!(a, Action::CopyStartPaused), false, false)?,
        "dual-confirm" => click(m, |a| matches!(a, Action::CopyStart), false, false)?,
        "dual-resume" => {
            let id = m.copy_id;
            m.action(Action::CopyPause(id, false));
        }
        "dual-close-panel" => m.action(Action::Tab(crate::tabs::Command::Close(m.tabs.active))),
        "dual-preferences" => m.action(Action::Settings),
        "dual-close" => m.closing = true,
        "dual-state" => {}
        _ => return Err("Comando dual desconocido".into()),
    }
    Ok(())
}
pub fn marker(m: &Model) -> serde_json::Value {
    let hex = |s: &std::ffi::OsStr| {
        s.as_encoded_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    let mut panels = vec![
        serde_json::json!({"id":m.dual.active_id,"revision":m.pane_revision,"active":true,"path":m.path.to_str(),"path_raw":hex(m.path.as_os_str()),"query":m.query,"scroll":m.scroll,"loaded":m.loaded&&!m.selection_refreshing(),"error":m.error,"selection":m.selection.iter().filter_map(|i|m.entries.get(*i)).map(|e|hex(&e.name)).collect::<Vec<_>>(),"order":m.order.iter().filter_map(|i|m.entries.get(*i)).map(|e|hex(&e.name)).collect::<Vec<_>>(),"sort":format!("{:?}",m.sort),"descending":m.descending,"summaries":m.summaries.len(),"stat_done":m.entries.iter().filter(|e|e.stat_attempted).count(),"tabs":m.tabs.rows.len(),"tab_id":m.tabs.active}),
    ];
    if let Some(b) = &m.dual.other {
        panels.push(serde_json::json!({"id":b.id,"revision":b.revision,"active":false,"path":b.path.to_str(),"path_raw":hex(b.path.as_os_str()),"query":b.query,"scroll":b.scroll,"loaded":b.loaded&&b.refresh_selection.is_none(),"error":b.error,"selection":b.selection.iter().filter_map(|i|b.entries.get(*i)).map(|e|hex(&e.name)).collect::<Vec<_>>(),"entries":b.entries.len(),"order":b.order.iter().filter_map(|i|b.entries.get(*i)).map(|e|hex(&e.name)).collect::<Vec<_>>(),"sort":format!("{:?}",b.sort),"descending":b.descending,"summaries":b.summaries.len(),"stat_done":b.entries.iter().filter(|e|e.stat_attempted).count(),"tabs":b.tabs.rows.len(),"tab_id":b.tabs.active}));
    }
    serde_json::json!({"enabled":m.dual.enabled,"right_active":m.dual.right_active,"panels":panels,"copy_form":m.copy_form.as_ref().map(|f|serde_json::json!({"source":hex(f.source.as_os_str()),"destination":hex(f.destination_path().as_os_str()),"names":f.names.iter().map(|n|hex(n)).collect::<Vec<_>>(),"moving":f.moving,"reviewed":f.prepared.is_some(),"start_paused":f.start_paused,"error":f.error})),"copy_busy":m.copy_busy(),"copy_id":m.copy_id,"copy_phase":format!("{:?}",m.copy.phase()),"copy_success":m.copy_last.as_ref().is_some_and(|o|o.result.is_ok()),"copy_owner":m.copy_owners.get(&m.copy_id),"operation":m.operation_message,"hits":m.hits.iter().filter_map(|h|{let Rect{x,y,w,h:height}=h.rect;match h.action{Action::PaneEntry(id,rev,i)=>Some(serde_json::json!({"panel":id,"revision":rev,"index":i,"rect":[x,y,w,height]})),_=>None}}).collect::<Vec<_>>()})
}
