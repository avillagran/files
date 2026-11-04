//! Fixture-only commands share real Actions/permission hit handlers, never OS input.
use crate::model::{Action, Model};
fn click(m: &mut Model, wanted: fn(&Action) -> bool) -> Result<(), String> {
    let r = m
        .hits
        .iter()
        .find(|h| h.generation == m.generation && wanted(&h.action))
        .map(|h| h.rect)
        .ok_or("visible property hit missing")?;
    m.pointer_down(r.x + r.w / 2, r.y + r.h / 2, false, false, false);
    Ok(())
}
pub fn command(m: &mut Model, command: &str) -> Result<(), String> {
    match command {
        "actions" => m.action(Action::Actions),
        "properties-file" | "properties-link" | "properties-folder" => {
            let name = match command {
                "properties-link" => "enlace",
                "properties-folder" => "carpeta",
                _ => "example-日本語.txt",
            };
            let index = m
                .entries
                .iter()
                .position(|e| e.name == name)
                .ok_or("fixture source missing")?;
            m.select_entry(index, false, false);
            click(m, |a| matches!(a, Action::Properties))?;
        }
        "focus-group-read" | "focus-other-read" => {
            let target = if command == "focus-group-read" { 3 } else { 6 };
            let d = m.properties_dialog.as_mut().ok_or("dialog missing")?;
            if d.body_height <= 0 {
                return Err("dialog has not been painted".into());
            }
            // Same focus/reveal routine used by Tab; next command clicks the
            // resulting fully visible target after an actual redraw.
            for _ in 0..9 {
                if d.focus == target {
                    break;
                }
                d.focus_next(false, d.body_height);
            }
        }
        "group-read" => click(m, |a| matches!(a, Action::PropertyBit(3)))?,
        "other-read" => click(m, |a| matches!(a, Action::PropertyBit(6)))?,
        "apply" | "apply-close" => {
            click(m, |a| matches!(a, Action::PropertyApply))?;
            if !m.mutation_applied {
                return Err("permissions were not submitted".into());
            }
            if command == "apply-close" {
                m.closing = true;
                m.mutations.cancel();
            }
        }
        "readonly-apply" => {
            let d = m.properties_dialog.as_ref().ok_or("dialog missing")?;
            if d.editable() || d.changed() {
                return Err("fixture must be read-only".into());
            }
            m.action(Action::PropertyBit(0));
            m.action(Action::PropertyApply);
            if m.mutation_applied {
                return Err("read-only object was applied".into());
            }
        }
        "reload" => click(m, |a| matches!(a, Action::PropertyReload))?,
        "dismiss" => click(m, |a| matches!(a, Action::PropertyClose))?,
        "close" => {
            m.close_properties();
            m.closing = true;
            m.mutations.cancel();
        }
        _ => return Err("unknown property fixture command".into()),
    }
    Ok(())
}
