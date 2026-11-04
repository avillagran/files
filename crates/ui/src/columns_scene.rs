//! Extension of the existing owned tabs fixture, not native keyboard/pointer input.
use crate::{
    model::{Action, Model, View},
    painter::Rect,
};
fn click(m: &mut Model, predicate: impl Fn(&Action) -> bool) -> Result<(), String> {
    let r = m
        .hits
        .iter()
        .rev()
        .find(|h| h.generation == m.generation && predicate(&h.action))
        .map(|h| h.rect)
        .ok_or("Control de Columnas no visible")?;
    m.pointer_down(r.x + r.w / 2, r.y + r.h / 2, false, false, false);
    Ok(())
}
pub fn command(m: &mut Model, action: &str, w: i32, h: i32) -> Result<(), String> {
    if let Some(name) = action.strip_prefix("columns-select:") {
        let i = m
            .entries
            .iter()
            .position(|e| e.name == name)
            .ok_or("Nombre fixture ausente")?;
        let id = m
            .columns
            .columns
            .get(m.columns.active)
            .ok_or("Columna fixture ausente")?
            .id;
        return click(m, |a| matches!(a,Action::ColumnEntry(c,j)if *c==id&&*j==i));
    }
    match action {
        "columns-view" => click(m, |a| matches!(a, Action::View(View::Columns)))?,
        "columns-right" => m.column_step(true),
        "columns-left" => m.column_step(false),
        "columns-root" => {
            let id = m.columns.columns.first().ok_or("Sin columna raíz")?.id;
            m.action(Action::ColumnFocus(id));
        }
        "columns-rapid" => {
            for name in ["A", "B"] {
                let i = m
                    .entries
                    .iter()
                    .position(|e| e.name == name)
                    .ok_or("Rama fixture ausente")?;
                m.select_entry(i, false, false);
                m.pump_columns();
            }
        }
        "columns-filter" => {
            crate::input(m, "same");
        }
        "columns-clear-filter" => {
            m.query.clear();
            m.rebuild();
        }
        "columns-raw" => {
            let i = m
                .entries
                .iter()
                .position(|e| e.name.as_encoded_bytes().contains(&0xff))
                .ok_or("Fixture raw ausente")?;
            m.select_entry(i, false, false);
        }
        "columns-copy-form" => {
            m.action(Action::CopyFiles);
            let f = m.copy_form.as_mut().ok_or("Formulario ausente")?;
            f.destination =
                std::env::var("FILES_TEST_COLUMNS_DEST").map_err(|_| "Destino privado ausente")?;
            f.replace = false;
        }
        "columns-copy-confirm" => click(m, |a| matches!(a, Action::CopyStart))?,
        "columns-rename" => m.action(Action::Rename),
        "columns-dismiss" => m.action(Action::RenameCancel),
        "columns-scroll-right" => click(m, |a| matches!(a,Action::ColumnScroll(d)if *d>0))?,
        "columns-scroll-left" => click(m, |a| matches!(a,Action::ColumnScroll(d)if *d<0))?,
        "columns-menu" => {
            if !crate::context_menu::open_keyboard(m, w, h) {
                return Err("Menú rechazado".into());
            }
        }
        "columns-close" => m.closing = true,
        "columns-state" => {}
        _ => return Err("Comando de Columnas desconocido".into()),
    }
    Ok(())
}
pub fn marker(m: &Model) -> serde_json::Value {
    let hex = |n: &std::ffi::OsStr| {
        n.as_encoded_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    serde_json::json!({"active":m.columns.active,"horizontal":m.columns.horizontal,"notice":m.columns.notice,"retained":m.columns.retained(),
 "rows":m.columns.columns.iter().enumerate().map(|(i,c)|serde_json::json!({"id":c.id,"watch_id":c.watch_id,"order":if i==m.columns.active {m.order.iter().map(|&i|hex(&m.entries[i].name)).collect::<Vec<_>>()}else{c.data.order.iter().map(|&i|hex(&c.data.entries[i].name)).collect::<Vec<_>>()},"selected":if i==m.columns.active {m.selected.and_then(|i|m.entries.get(i)).map(|e|hex(&e.name))}else{c.data.selected.and_then(|i|c.data.entries.get(i)).map(|e|hex(&e.name))},"selection":if i==m.columns.active {m.selection.iter().filter_map(|&i|m.entries.get(i)).map(|e|hex(&e.name)).collect::<Vec<_>>()}else{c.data.selection.iter().filter_map(|&i|c.data.entries.get(i)).map(|e|hex(&e.name)).collect::<Vec<_>>()},"scroll":if i==m.columns.active{m.scroll}else{c.data.scroll},"path":c.path.to_str(),"path_raw":hex(c.path.as_os_str()),"loaded":if i==m.columns.active{m.loaded}else{c.data.loaded},"entries":if i==m.columns.active{m.entries.len()}else{c.data.entries.len()},"error":c.data.error})).collect::<Vec<_>>(),
 "copy_form":m.copy_form.as_ref().map(|f|serde_json::json!({"source":f.source.to_str(),"names":f.names.iter().map(|n|hex(n)).collect::<Vec<_>>(),"destination":f.destination})),"copy_busy":m.copy_busy(),"operation":m.operation_message,"copy_success":m.copy_last.as_ref().is_some_and(|o|o.result.is_ok()),
 "rename_names":m.rename_dialog.as_ref().map(|d|d.selected.iter().map(|n|hex(n)).collect::<Vec<_>>()),
 "row_hits":m.hits.iter().filter_map(|h|if let Action::ColumnEntry(id,i)=h.action{let Rect{x,y,w,h}=h.rect;Some(serde_json::json!({"column":id,"index":i,"rect":[x,y,w,h]}))}else{None}).collect::<Vec<_>>()})
}
