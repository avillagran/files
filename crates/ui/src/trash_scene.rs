//! Opt-in fixture handlers. Mutates only paths selected from fixture entries.
use crate::model::{Action, Model};
use std::ffi::OsStr;
fn select(m: &mut Model, predicate: impl Fn(&OsStr) -> bool) -> Result<(), &'static str> {
    let indices = m
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| predicate(&e.name))
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    if indices.is_empty() {
        return Err("La fixture no contiene la selección pedida");
    }
    m.selection.clear();
    m.selection.extend(indices.iter().copied());
    m.selected = indices.first().copied();
    m.anchor = m.selected;
    Ok(())
}
fn click(m: &mut Model, predicate: impl Fn(&Action) -> bool) -> Result<(), &'static str> {
    let hit = m
        .hits
        .iter()
        .rev()
        .find(|hit| hit.generation == m.generation && predicate(&hit.action))
        .ok_or("Target de acción no visible")?;
    let (x, y) = (hit.rect.x + hit.rect.w / 2, hit.rect.y + hit.rect.h / 2);
    m.pointer_down(x, y, false, false, false);
    Ok(())
}
pub fn command(m: &mut Model, text: &str) -> Result<(), &'static str> {
    match text {
        "actions" => click(m, |a| matches!(a, Action::Actions))?,
        "trash-basic" => {
            select(m, |n| {
                ["uno.txt", "dos.txt", "carpeta", "enlace"]
                    .iter()
                    .any(|v| n == OsStr::new(v))
            })?;
            click(m, |a| matches!(a, Action::Trash))?;
            if !m.mutation_applied {
                return Err("La tarea de Papelera no se inició");
            }
        }
        "trash-raw" => {
            select(m, |n| n.as_encoded_bytes() == b"raw\xff.bin")?;
            click(m, |a| matches!(a, Action::Trash))?;
            if !m.mutation_applied {
                return Err("La tarea raw no se inició");
            }
        }
        "trash-many" => {
            select(m, |n| n.to_str().is_some_and(|n| n.starts_with("many-")))?;
            click(m, |a| matches!(a, Action::Trash))?;
        }
        "restore" => {
            click(m, |a| matches!(a, Action::Undo(false)))?;
            if !m.mutation_applied {
                return Err("Restaurar no se inició");
            }
        }
        "report-pan" => {
            if !m.notice_open || m.notice_width == 0 {
                return Err("No hay informe visible para desplazar");
            }
            crate::mutations::pan_notice(m, m.notice_width);
        }
        "reload" => m.refresh_current(),
        "dismiss" => m.notice_open = false,
        "close" => {
            eprintln!(
                "FILES_TRASH_CLOSE busy={} applied={} id={}",
                m.mutations.busy, m.mutation_applied, m.mutation_id
            );
            m.closing = true;
        }
        _ => return Err("Comando de Papelera desconocido"),
    }
    Ok(())
}
