//! Owned-fixture actions, never native input. Only enabled by the smoke harness.
use crate::{
    model::{Action, Model, View},
    tabs::Command,
};
use std::path::Path;
fn click(m: &mut Model, predicate: impl Fn(&Action) -> bool) -> Result<(), String> {
    let r = m
        .hits
        .iter()
        .rev()
        .find(|h| h.generation == m.generation && predicate(&h.action))
        .map(|h| h.rect)
        .ok_or("Control de pestaña no visible")?;
    m.pointer_down(r.x + r.w / 2, r.y + r.h / 2, false, false, false);
    Ok(())
}
fn select(m: &mut Model, name: &str, toggle: bool) -> Result<(), String> {
    let i = m
        .entries
        .iter()
        .position(|e| e.name == name)
        .ok_or("Entrada fixture ausente")?;
    m.select_entry(i, toggle, false);
    Ok(())
}
pub fn command(m: &mut Model, root: &Path, action: &str) -> Result<(), String> {
    match action {
        "tab-session-preferences" => m.action(Action::Settings),
        "tab-session-save" => click(m, |a| matches!(a, Action::SaveSession))?,
        "tab-session-close" => m.closing = true,
        "tab-session-state" => {}
        "tab-a-location" => m.action(Action::Navigate(root.to_path_buf())),
        "tab-new" => click(m, |a| matches!(a, Action::Tab(Command::New)))?,
        "tab-a" | "tab-b" => {
            let at = usize::from(action == "tab-b");
            let id = m.tabs.rows.get(at).ok_or("Pestaña fixture ausente")?.id;
            if !m
                .hits
                .iter()
                .any(|h| matches!(h.action,Action::Tab(Command::Select(i))if i==id))
            {
                m.action(Action::Tab(Command::Select(id)));
            } else {
                click(m, |a| matches!(a,Action::Tab(Command::Select(i))if *i==id))?;
            }
        }
        "tab-b-location" => m.action(Action::Navigate(
            root.parent().ok_or("Raíz fixture ausente")?.join("B"),
        )),
        "tab-config-a" => {
            m.view = View::Grid;
            m.grid_zoom = 1.25;
            m.sort = files_core::listing::Sort::Name;
            m.descending = true;
            m.query = "keep".into();
            m.rebuild();
            select(m, "keep.txt", false)?;
            select(m, "keeper.txt", true)?;
            m.scroll = 120;
            m.pan_x = 48.;
        }
        "tab-config-b" => {
            m.view = View::List;
            m.query = "same".into();
            m.rebuild();
            select(m, "same.txt", false)?;
        }
        "tab-rapid" => {
            let a = m.tabs.rows[0].id;
            let b = m.tabs.rows[1].id;
            for id in [a, b, a, b, a] {
                m.action(Action::Tab(Command::Select(id)));
            }
        }
        "tab-close" => m.action(Action::Tab(Command::Close(m.tabs.active))),
        "tab-reopen" => m.action(Action::Tab(Command::Reopen)),
        "tab-cycle" => m.action(Action::Tab(Command::Cycle(1))),
        "tab-hidden-select" => {
            m.query.clear();
            if !m.show_hidden {
                m.action(Action::ToggleHidden);
            }
            m.rebuild();
            select(m, ".hidden", false)?;
        }
        "tab-close-others" => {
            let ids = m
                .tabs
                .rows
                .iter()
                .map(|t| t.id)
                .filter(|id| *id != m.tabs.active)
                .collect::<Vec<_>>();
            for id in ids {
                m.action(Action::Tab(Command::Close(id)));
            }
        }
        "tab-hidden" => m.action(Action::ToggleHidden),
        "tab-preview" => {
            m.query.clear();
            m.rebuild();
            select(m, "same.txt", false)?;
            m.action(Action::Preview);
        }
        "tab-preview-b" => {
            let crate::text_preview_state::View::Ready(r) = m.preview.view() else {
                return Err("Texto B no listo".into());
            };
            if r.document.text() != "B content\n" {
                return Err("Texto de otra pestaña".into());
            }
        }
        "tab-close-preview" => m.action(Action::ClosePreview),
        "tab-raw-select" => {
            let i = m
                .entries
                .iter()
                .position(|e| e.name.as_encoded_bytes().contains(&0xff))
                .ok_or("Nombre raw ausente")?;
            m.query.clear();
            m.rebuild();
            m.select_entry(i, false, false);
        }
        "tab-directory" => {
            m.query.clear();
            m.rebuild();
            select(m, "folder", false)?;
        }
        "tab-file" => {
            m.query.clear();
            m.rebuild();
            select(m, "same.txt", false)?;
        }
        "tab-list" => m.action(Action::View(View::List)),
        _ => return Err("Comando de pestañas desconocido".into()),
    }
    Ok(())
}
