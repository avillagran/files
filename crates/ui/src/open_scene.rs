//! Opt-in fixture commands: visible Open hit handlers, never native OS input.
//! The window callback command is owned by lib.rs, not synthesized here.
use crate::model::{Action, Model};
use std::ffi::OsStr;

pub fn command(model: &mut Model, action: &str) -> Result<(), String> {
    if !model.loaded || model.open_job.is_some() {
        return Err("La fixture de apertura aún no está lista".into());
    }
    if action == "actions" {
        model.action(Action::Actions);
        return Ok(());
    }
    let name = match action {
        "file" => "example-日本語 $().txt",
        "link" => "enlace",
        "folder" => "childdir",
        _ => return Err("Comando de apertura desconocido".into()),
    };
    let index = model
        .entries
        .iter()
        .position(|entry| entry.name == OsStr::new(name))
        .ok_or("No existe el elemento de la fixture de apertura")?;
    let directory = model.entries[index].is_dir();
    if directory != (action == "folder") {
        return Err("El tipo del elemento de apertura no coincide con la fixture".into());
    }
    model.select_entry(index, false, false);
    let hit = model
        .hits
        .iter()
        .rev()
        .find(|hit| hit.generation == model.generation && matches!(hit.action, Action::Open))
        .map(|hit| hit.rect)
        .filter(|rect| rect.w > 0 && rect.h > 0)
        .ok_or("No hay un botón Abrir visible en la fixture")?;
    model.pointer_down(hit.x + hit.w / 2, hit.y + hit.h / 2, false, false, false);
    if !directory && model.open_job.is_none() {
        return Err("El botón Abrir no inició el comando del sistema".into());
    }
    Ok(())
}
