//! Explicit fixture-only actions for reproducible framebuffer and filesystem checks.
use crate::{
    model::{Action, Model},
    rename_dialog::Mode,
};
pub fn command(m: &mut Model, command: &str) -> Result<(), String> {
    match command {
        "rename-preview" => {
            if !m.loaded || m.mutations.busy {
                return Err("scene is not ready".into());
            }
            let mut indices = Vec::new();
            for name in ["photo1.txt", "photo2.txt"] {
                indices.push(
                    m.entries
                        .iter()
                        .position(|e| e.name == name)
                        .ok_or("rename fixture missing source")?,
                );
            }
            m.select_entry(indices[0], false, false);
            m.select_entry(indices[1], true, false);
            m.open_rename(Mode::Rename);
            m.rename_dialog
                .as_mut()
                .ok_or("rename dialog missing")?
                .input("image-{n:03}{dotext}");
        }
        "regex-preview" | "regex-invalid" | "regex-expand" => {
            if m.mutations.busy {
                return Err("worker is busy".into());
            }
            if command == "regex-preview" {
                let mut indices = Vec::new();
                for name in ["photo1.txt", "photo2.txt"] {
                    indices.push(
                        m.entries
                            .iter()
                            .position(|e| e.name == name)
                            .ok_or("regex fixture missing source")?,
                    );
                }
                m.select_entry(indices[0], false, false);
                m.select_entry(indices[1], true, false);
                m.open_rename(Mode::Rename);
                m.action(Action::RenameRegex);
            }
            let d = m.rename_dialog.as_mut().ok_or("rename dialog missing")?;
            d.fields[0] = "{name}{dotext}".into();
            d.fields[3] = match command {
                "regex-invalid" => "(",
                "regex-expand" => "(?<x>.)",
                _ => "photo(?<n>[0-9]+)",
            }
            .into();
            d.fields[4] = if command == "regex-expand" {
                format!("{}$x", "x".repeat(1024))
            } else {
                "imagen-${n}".into()
            };
            d.focus = 3;
            d.replace = true;
            d.change();
        }
        "create-file" | "create-folder" => {
            m.open_rename(if command == "create-file" {
                Mode::File
            } else {
                Mode::Folder
            });
            m.rename_dialog
                .as_mut()
                .ok_or("create dialog missing")?
                .input(if command == "create-file" {
                    "created.txt"
                } else {
                    "Created"
                });
        }
        "apply-invalid" => {
            if m.rename_dialog
                .as_ref()
                .is_none_or(|d| d.prepared.is_some() || d.error.is_empty())
            {
                return Err("fixture must be an invalid preview".into());
            }
            m.action(Action::RenameApply);
            if m.mutation_applied {
                return Err("invalid plan was applied".into());
            }
        }
        "apply" | "apply-close" => {
            m.apply_rename();
            if !m.mutation_applied {
                return Err("nothing was applied".into());
            }
            if command == "apply-close" {
                m.closing = true;
                m.mutations.cancel();
            }
        }
        "undo" | "redo" => {
            m.undo_rename(command == "redo");
            if !m.mutation_applied {
                return Err("history unavailable".into());
            }
        }
        "continue" => m.action(Action::DismissNotice),
        "close" => {
            m.closing = true;
            m.mutations.cancel();
        }
        _ => return Err("unknown fixture action".into()),
    }
    Ok(())
}
