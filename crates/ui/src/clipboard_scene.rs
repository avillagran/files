//! Extension of the existing tabs fixture: shared targets, not native input.
use crate::model::{Action, Model};
fn click(m: &mut Model, predicate: impl Fn(&Action) -> bool, toggle: bool) -> Result<(), String> {
    let r = m
        .hits
        .iter()
        .rev()
        .find(|h| h.generation == m.generation && predicate(&h.action))
        .map(|h| h.rect)
        .ok_or("Control del portapapeles no visible")?;
    m.pointer_down(r.x + r.w / 2, r.y + r.h / 2, toggle, false, false);
    Ok(())
}
pub fn command(m: &mut Model, action: &str, w: i32, h: i32) -> Result<(), String> {
    if let Some(hex) = action
        .strip_prefix("clip-select:")
        .or_else(|| action.strip_prefix("clip-toggle:"))
    {
        let (pairs, remainder) = hex.as_bytes().as_chunks::<2>();
        if !remainder.is_empty() {
            return Err("Hex inválido".into());
        }
        let name = pairs
            .iter()
            .map(|b| {
                std::str::from_utf8(b)
                    .ok()
                    .and_then(|s| u8::from_str_radix(s, 16).ok())
            })
            .collect::<Option<Vec<_>>>()
            .ok_or("Hex inválido")?;
        let i = m
            .entries
            .iter()
            .position(|e| e.name.as_encoded_bytes() == name)
            .ok_or("Nombre fixture ausente")?;
        return click(
            m,
            |a| matches!(a,Action::Entry(j)if *j==i),
            action.starts_with("clip-toggle:"),
        );
    }
    match action {
        "clip-copy" | "clip-cut" | "clip-paste" | "clip-system-paste" => {
            let index = m
                .context_menu
                .as_ref()
                .and_then(|menu| {
                    menu.items.iter().position(|i| {
                        matches!(
                            (&i.action, action),
                            (Some(Action::ClipboardCopy), "clip-copy")
                                | (Some(Action::ClipboardCut), "clip-cut")
                                | (Some(Action::ClipboardPaste), "clip-paste")
                                | (Some(Action::NativePaste), "clip-system-paste")
                        )
                    })
                })
                .ok_or("Acción contextual ausente/deshabilitada")?;
            click(m, |a| matches!(a,Action::ContextItem(i)if *i==index), false)?;
        }
        "clip-menu" => {
            if !crate::context_menu::open_keyboard(m, w, h) {
                return Err("Menú rechazado".into());
            }
        }
        "clip-menu-close" => crate::context_menu::dismiss(m),
        #[cfg(target_os = "linux")]
        "clip-dnd-send" => m.fixture_outgoing_drag()?,
        #[cfg(target_os = "linux")]
        "clip-kde-cut" => {
            use std::{io::Write, os::fd::FromRawFd};
            let source = std::path::PathBuf::from(
                std::env::var_os("FILES_TEST_CLIP_A").ok_or("Fixture source missing")?,
            )
            .join("changed");
            let wire = files_core::file_transfer::encode(
                files_core::file_transfer::Format::UriList,
                files_core::file_transfer::Intent::Copy,
                &[source],
            )?;
            fn private_pipe(data: &[u8]) -> Result<std::fs::File, String> {
                let mut fds = [0; 2];
                if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                let read = unsafe { std::fs::File::from_raw_fd(fds[0]) };
                let mut write = unsafe { std::fs::File::from_raw_fd(fds[1]) };
                let flags = unsafe { libc::fcntl(fds[0], libc::F_GETFL) };
                if flags < 0
                    || unsafe { libc::fcntl(fds[0], libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
                {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                write.write_all(data).map_err(|e| e.to_string())?;
                drop(write);
                Ok(read)
            }
            if wire.len() > 4096 {
                return Err("Fixture KDE excede capacidad".into());
            }
            m.fixture_kde_import(private_pipe(&wire)?, private_pipe(b"1")?)?;
        }
        #[cfg(target_os = "linux")]
        "clip-dnd-hover" => m.fixture_drag(None)?,
        #[cfg(target_os = "linux")]
        "clip-dnd-drop" => {
            use std::{
                io::Write,
                os::{fd::FromRawFd, unix::ffi::OsStringExt},
            };
            let source = std::path::PathBuf::from(
                std::env::var_os("FILES_TEST_CLIP_A").ok_or("Fixture source missing")?,
            )
            .join(std::ffi::OsString::from_vec(b"dnd-\xff".to_vec()));
            let wire = files_core::file_transfer::encode(
                files_core::file_transfer::Format::UriList,
                files_core::file_transfer::Intent::Copy,
                &[source],
            )?;
            if wire.len() > 4096 {
                return Err("Fixture wire exceeds pipe capacity".into());
            }
            let mut fds = [0; 2];
            if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let reader = unsafe { std::fs::File::from_raw_fd(fds[0]) };
            let mut writer = unsafe { std::fs::File::from_raw_fd(fds[1]) };
            let flags = unsafe { libc::fcntl(fds[0], libc::F_GETFL) };
            if flags < 0
                || unsafe { libc::fcntl(fds[0], libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                return Err(std::io::Error::last_os_error().to_string());
            }
            writer.write_all(&wire).map_err(|e| e.to_string())?;
            drop(writer);
            m.fixture_drag(Some(reader))?;
        }
        "clip-actions" => click(m, |a| matches!(a, Action::Actions), false)?,
        "clip-actions-paste" => click(m, |a| matches!(a, Action::ClipboardPaste), false)?,
        "clip-confirm" => click(m, |a| matches!(a, Action::CopyStart), false)?,
        "clip-paused" => click(m, |a| matches!(a, Action::CopyStartPaused), false)?,
        "clip-cancel-dialog" => click(m, |a| matches!(a, Action::CopyDialogCancel), false)?,
        "clip-cancel-job" => {
            let id = m.copy_id;
            click(m, |a| matches!(a,Action::CopyCancel(i)if *i==id), false)?;
        }
        "clip-resume" => {
            let id = m.copy_id;
            click(
                m,
                |a| matches!(a,Action::CopyPause(i,false)if *i==id),
                false,
            )?;
        }
        "clip-dismiss" => m.action(Action::DismissNotice),
        "clip-a" | "clip-b" => m.navigate(
            std::env::var_os(if action == "clip-a" {
                "FILES_TEST_CLIP_A"
            } else {
                "FILES_TEST_CLIP_B"
            })
            .ok_or("Ruta privada ausente")?
            .into(),
        ),
        "clip-text-own" => {
            m.action(Action::Omni);
            fltk::app::copy("texto á\tb");
        }
        "clip-text-cancel" => {
            m.end_path_edit();
            m.text_paste.invalidate();
        }
        "clip-close" => m.closing = true,
        "clip-state" => {}
        _ => return Err("Comando clipboard desconocido".into()),
    }
    Ok(())
}
