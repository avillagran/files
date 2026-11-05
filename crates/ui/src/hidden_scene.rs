//! Explicit fixture-only handlers and lossless visibility evidence. No OS input.
use crate::{
    context_menu,
    model::{Action, Model, View},
};
use fltk::enums::Key;
fn click(m: &mut Model, predicate: impl Fn(&Action) -> bool) -> Result<(), String> {
    let r = m
        .hits
        .iter()
        .rev()
        .find(|h| h.generation == m.generation && predicate(&h.action))
        .map(|h| h.rect)
        .ok_or("Control no visible")?;
    m.pointer_down(r.x + r.w / 2, r.y + r.h / 2, false, false, false);
    Ok(())
}
fn index(m: &Model, name: &str) -> Result<usize, String> {
    m.entries
        .iter()
        .position(|e| e.name == name)
        .ok_or_else(|| format!("Entrada fixture ausente: {name}"))
}
pub fn command(m: &mut Model, action: &str, w: i32, h: i32) -> Result<(), String> {
    if let Some(command) = action.strip_prefix("sftp-") {
        use crate::sftp_ui::Command as C;
        match command {
            "open" => click(m, |a| matches!(a, Action::Sftp))?,
            "report" => click(m, |a| matches!(a, Action::SftpCommand(_, C::Report)))?,
            "report-copy" => click(m, |a| matches!(a, Action::SftpCommand(_, C::ReportCopy(_))))?,
            "report-refresh" => click(m, |a| {
                matches!(a, Action::SftpCommand(_, C::ReportRefresh(_)))
            })?,
            "report-back" => click(m, |a| matches!(a, Action::SftpCommand(_, C::ReportBack(_))))?,
            "report-next" => click(m, |a| {
                matches!(
                    a,
                    Action::SftpCommand(
                        _,
                        C::ReportNav(_, crate::notice_navigation::Command::Pages(1))
                    )
                )
            })?,
            "report-prev" => click(m, |a| {
                matches!(
                    a,
                    Action::SftpCommand(
                        _,
                        C::ReportNav(_, crate::notice_navigation::Command::Pages(-1))
                    )
                )
            })?,
            "report-end" => click(m, |a| {
                matches!(
                    a,
                    Action::SftpCommand(_, C::ReportNav(_, crate::notice_navigation::Command::End))
                )
            })?,
            "fixture-long-diagnostic" => {
                m.sftp.transport.status.diagnostic = format!(
                    "BEGIN-OWNED\n{}\nEND-OWNED",
                    "línea ñ /tmp/%41/$()\t\u{1b}".repeat(120)
                );
                m.sftp.transport.status.phase = "Uncertain".into();
            }
            "fixture-new-diagnostic" => {
                m.sftp.transport.status.diagnostic = "NEW-OBSERVATION-OWNED".into()
            }
            "profiles" => click(m, |a| matches!(a, Action::SftpCommand(_, C::Profiles)))?,
            "profile-save" => click(m, |a| matches!(a, Action::SftpCommand(_, C::ProfileSave)))?,
            "profile-load" => click(m, |a| {
                matches!(a, Action::SftpCommand(_, C::ProfileLoad(..)))
            })?,
            "profile-remove" => click(m, |a| {
                matches!(a, Action::SftpCommand(_, C::ProfileRemove(..)))
            })?,
            "profile-refresh" => click(m, |a| {
                matches!(a, Action::SftpCommand(_, C::ProfileRefresh))
            })?,
            "profile-discard" => click(m, |a| {
                matches!(a, Action::SftpCommand(_, C::ProfileDiscard))
            })?,
            "profile-back" => click(m, |a| matches!(a, Action::SftpCommand(_, C::ProfileBack)))?,
            "review" => click(m, |a| matches!(a, Action::SftpCommand(_, C::Review)))?,
            "connect" => click(m, |a| matches!(a, Action::SftpCommand(_, C::Connect)))?,
            "mount-open" => click(m, |a| matches!(a, Action::SftpCommand(_, C::Open)))?,
            "disconnect" => click(m, |a| matches!(a, Action::SftpCommand(_, C::Disconnect)))?,
            "reconcile" => click(m, |a| matches!(a, Action::SftpCommand(_, C::Reconcile)))?,
            "sync" => click(m, |a| matches!(a, Action::SftpCommand(_, C::Sync)))?,
            "check" => click(m, |a| matches!(a, Action::SftpCommand(_, C::Check)))?,
            "close" => click(m, |a| matches!(a, Action::SftpCommand(_, C::Close)))?,
            "request-paste" => {
                m.text_paste
                    .request(m.text_context())
                    .ok_or("Paste gate busy")?;
            }
            "late-paste" => {
                if m.receive_text_paste("STALE") {
                    return Err("Stale SFTP paste accepted".into());
                }
            }
            _ => {
                if let Some(text) = command.strip_prefix("input:") {
                    m.sftp_input(text);
                } else if let Some(i) = command.strip_prefix("focus:") {
                    let i: usize = i.parse().map_err(|_| "Bad SFTP field")?;
                    click(
                        m,
                        |a| matches!(a,Action::SftpCommand(_,C::Focus(f))if *f==i),
                    )?;
                } else if let Some(id) = command.strip_prefix("report-old-copy:") {
                    let stamp = m.sftp.form.as_ref().ok_or("SFTP form absent")?.stamp;
                    m.action(Action::SftpCommand(
                        stamp,
                        C::ReportCopy(id.parse().map_err(|_| "Bad report id")?),
                    ));
                } else if let Some(stamp) = command.strip_prefix("old-close:") {
                    m.action(Action::SftpCommand(
                        stamp.parse().map_err(|_| "Bad SFTP stamp")?,
                        C::Close,
                    ));
                } else if let Some(name) = command.strip_prefix("key:") {
                    let (k, mods) = match name {
                        "report" => (Key::F4, fltk::enums::Shortcut::None),
                        "copy-report" => (Key::from_char('c'), fltk::enums::Shortcut::Ctrl),
                        "page-down" => (Key::PageDown, fltk::enums::Shortcut::None),
                        "page-up" => (Key::PageUp, fltk::enums::Shortcut::None),
                        "home" => (Key::Home, fltk::enums::Shortcut::None),
                        "refresh-report" => (Key::F5, fltk::enums::Shortcut::None),
                        "profiles" => (Key::F2, fltk::enums::Shortcut::None),
                        "save" => (Key::from_char('s'), fltk::enums::Shortcut::Ctrl),
                        "down" => (Key::Down, fltk::enums::Shortcut::None),
                        "up" => (Key::Up, fltk::enums::Shortcut::None),
                        "escape" => (Key::Escape, fltk::enums::Shortcut::None),
                        "ctrl1" => (Key::from_char('1'), fltk::enums::Shortcut::Ctrl),
                        "ctrl2" => (Key::from_char('2'), fltk::enums::Shortcut::Ctrl),
                        "ctrl3" => (Key::from_char('3'), fltk::enums::Shortcut::Ctrl),
                        "ctrl4" => (Key::from_char('4'), fltk::enums::Shortcut::Ctrl),
                        "all" => (Key::from_char('a'), fltk::enums::Shortcut::Ctrl),
                        "end" => (Key::End, fltk::enums::Shortcut::None),
                        "left" => (Key::Left, fltk::enums::Shortcut::None),
                        "undo" => (Key::from_char('z'), fltk::enums::Shortcut::Ctrl),
                        "alt-up" => (Key::Up, fltk::enums::Shortcut::Alt),
                        "review" => (Key::Enter, fltk::enums::Shortcut::None),
                        _ => return Err("Unknown SFTP key".into()),
                    };
                    crate::sftp_ui::key(m, k, mods, "");
                } else {
                    return Err("Unknown SFTP fixture command".into());
                }
            }
        }
        return Ok(());
    }
    if action.starts_with("saved-") {
        return crate::saved_search_ui::fixture_command(m, action).map_err(str::to_string);
    }
    if let Some(command) = action.strip_prefix("dest-") {
        use fltk::enums::Shortcut;
        if let Some(name) = command.strip_prefix("report-key:") {
            let key = match name {
                "up" => Key::Up,
                "down" => Key::Down,
                "page-up" => Key::PageUp,
                "page-down" => Key::PageDown,
                "home" => Key::Home,
                "end" => Key::End,
                "left" => Key::Left,
                "right" => Key::Right,
                _ => return Err("Tecla de informe desconocida".into()),
            };
            if !crate::mutations::notice_shortcut(m, key, false) {
                return Err("Informe no activo".into());
            }
            return Ok(());
        }
        match command {
            "report-next" => click(m, |a| {
                matches!(
                    a,
                    Action::NoticeNavigate(crate::notice_navigation::Command::Pages(1))
                )
            })?,
            "report-prev" => click(m, |a| {
                matches!(
                    a,
                    Action::NoticeNavigate(crate::notice_navigation::Command::Pages(-1))
                )
            })?,
            "report-start" => click(m, |a| {
                matches!(
                    a,
                    Action::NoticeNavigate(crate::notice_navigation::Command::Start)
                )
            })?,
            "report-end" => click(m, |a| {
                matches!(
                    a,
                    Action::NoticeNavigate(crate::notice_navigation::Command::End)
                )
            })?,
            "copy" | "move" => {
                let i = index(m, "note.txt")?;
                m.select_entry(i, false, false);
                m.action(if command == "move" {
                    Action::MoveFiles
                } else {
                    Action::CopyFiles
                });
            }
            "name-focus" => click(m, |a| matches!(a, Action::CopyNameFocus(_)))?,
            "name-next" => click(m, |a| matches!(a, Action::CopyNameStep(_, 1)))?,
            "name-prev" => click(m, |a| matches!(a, Action::CopyNameStep(_, -1)))?,
            "name-reset" => click(m, |a| matches!(a, Action::CopyNameReset(_)))?,
            "name-point-end" => {
                let r = m
                    .hits
                    .iter()
                    .find(|h| matches!(h.action, Action::CopyNameFocus(_)))
                    .ok_or("name field absent")?
                    .rect;
                crate::copy_ui::destination_pointer(
                    m,
                    r.x + r.w - 2,
                    r.y + r.h / 2,
                    true,
                    false,
                    false,
                    false,
                );
                crate::copy_ui::destination_pointer(m, 0, 0, false, false, false, false);
            }
            "name-oversize" => crate::copy_ui::destination_key(
                m,
                Key::None,
                Shortcut::None,
                &"x".repeat(1024 * 1024),
            ),
            "copy-selected" => m.action(Action::CopyFiles),
            "copy-a" => {
                let i = index(m, "a.txt")?;
                m.select_entry(i, false, false);
                m.action(Action::CopyFiles);
            }
            "private-copy-a" => {
                let i = index(m, "a.txt")?;
                m.select_entry(i, false, false);
                m.action(Action::ClipboardCopy);
            }
            "private-paste" => m.action(Action::ClipboardPaste),
            "copy-all" => {
                m.action(Action::Selection(crate::selection_tools::Command::All));
                m.action(Action::CopyFiles);
            }
            "skip" => click(m, |a| matches!(a, Action::CopySkipExisting))?,
            "options" => click(m, |a| matches!(a, Action::CopyOptions))?,
            "visible" => click(m, |a| matches!(a, Action::CopyVisiblePublication))?,
            "symlink-times" => click(m, |a| matches!(a, Action::CopySymlinkTimes))?,
            "start-paused" => click(m, |a| matches!(a, Action::CopyStartPaused))?,
            "resume" => click(m, |a| matches!(a, Action::CopyPause(_, false)))?,
            "cancel-active" => click(m, |a| matches!(a, Action::CopyCancel(_)))?,
            "retry-key" => {
                crate::mutations::notice_shortcut(m, Key::from_char('r'), true);
            }
            "retry-pending" => click(m, |a| matches!(a, Action::RetryPendingMove))?,
            "details" => click(m, |a| matches!(a, Action::CopyDetails))?,
            "dismiss" => click(m, |a| matches!(a, Action::DismissNotice))?,
            "start" => click(m, |a| matches!(a, Action::CopyStart))?,
            "request-paste" => {
                let context = m.text_context();
                m.text_paste
                    .request(context)
                    .ok_or("text receipt not admitted")?;
            }
            "late-paste" => {
                if m.receive_text_paste("STALE") {
                    return Err("stale text accepted".into());
                }
            }
            "oversize" => m
                .copy_form
                .as_mut()
                .ok_or("form absent")?
                .input(&"x".repeat(1024 * 1024)),
            #[cfg(unix)]
            "raw" => {
                use std::os::unix::ffi::OsStringExt;
                m.copy_form
                    .as_mut()
                    .ok_or("form absent")?
                    .capture_destination(
                        std::ffi::OsString::from_vec(b"/owned-destination-\xff".to_vec()).into(),
                    );
            }
            #[cfg(not(unix))]
            "raw" => return Err("Raw Unix fixture unsupported".into()),
            "limit" => m
                .copy_form
                .as_mut()
                .ok_or("form absent")?
                .input(&format!("/{}", "x".repeat(4095))),
            "point-end" => {
                let r = m
                    .hits
                    .iter()
                    .find(|h| matches!(h.action, Action::CopyDestination))
                    .ok_or("destination not painted")?
                    .rect;
                crate::copy_ui::destination_pointer(
                    m,
                    r.x + r.w - 2,
                    r.y + r.h / 2,
                    true,
                    false,
                    false,
                    false,
                );
            }
            _ => {
                if let Some(text) = command.strip_prefix("name-text:") {
                    crate::copy_ui::destination_key(m, Key::None, Shortcut::None, text);
                } else if let Some(text) = command.strip_prefix("text:") {
                    m.copy_form.as_mut().ok_or("form absent")?.input(text);
                } else if let Some(key) = command.strip_prefix("key:") {
                    let (k, mods) = match key {
                        "home" => (Key::Home, Shortcut::None),
                        "end" => (Key::End, Shortcut::None),
                        "shift-home" => (Key::Home, Shortcut::Shift),
                        "altleft" => (Key::Left, Shortcut::Alt),
                        "altright" => (Key::Right, Shortcut::Alt),
                        "altup" => (Key::Up, Shortcut::Alt),
                        "left" => (Key::Left, Shortcut::None),
                        "right" => (Key::Right, Shortcut::None),
                        "all" => (Key::from_char('a'), Shortcut::Ctrl),
                        "undo" => (Key::from_char('z'), Shortcut::Ctrl),
                        "redo" => (Key::from_char('y'), Shortcut::Ctrl),
                        "skip" => (Key::F4, Shortcut::None),
                        "options" => (Key::F3, Shortcut::None),
                        "visible" => (Key::from_char('1'), Shortcut::None),
                        "symlink-times" => (Key::from_char('2'), Shortcut::None),
                        "enter" => (Key::Enter, Shortcut::None),
                        "name-focus" => (Key::F2, Shortcut::None),
                        "folder-focus" => (Key::from_char('l'), Shortcut::Ctrl),
                        "name-next" => (Key::Down, Shortcut::Alt),
                        "name-prev" => (Key::Up, Shortcut::Alt),
                        "escape" => (Key::Escape, Shortcut::None),
                        _ => return Err("unknown destination key".into()),
                    };
                    crate::copy_ui::destination_key(m, k, mods, "");
                } else {
                    return Err("unknown destination fixture command".into());
                }
            }
        }
        return Ok(());
    }
    if action.starts_with("book-") {
        use crate::bookmarks_ui::{Command as B, Row};
        match action {
            "book-open" => click(m, |a| matches!(a, Action::Bookmarks))?,
            "book-close" => click(m, |a| matches!(a, Action::Bookmark(_, B::Close)))?,
            "book-add" => click(m, |a| matches!(a, Action::Bookmark(_, B::Add)))?,
            "book-edit" => click(m, |a| matches!(a, Action::Bookmark(_, B::Edit)))?,
            "book-save" => click(m, |a| matches!(a, Action::Bookmark(_, B::SaveLabel)))?,
            "book-remove" => click(m, |a| matches!(a, Action::Bookmark(_, B::Remove)))?,
            "book-reload" => click(m, |a| matches!(a, Action::Bookmark(_, B::Reload)))?,
            "book-omit" => click(m, |a| matches!(a, Action::Bookmark(_, B::Discard)))?,
            "book-details" => click(m, |a| matches!(a, Action::Bookmark(_, B::Details)))?,
            "book-jump" => click(m, |a| matches!(a, Action::Bookmark(_, B::Jump)))?,
            "book-cancel-close" => m.action(Action::CancelClose),
            "book-fixture-places" => {
                let target =
                    std::env::var_os("FILES_TEST_BOOKMARK_PLACE").ok_or("Lugar privado ausente")?;
                m.places = vec![files_core::places::Place {
                    label: "Destino privado".into(),
                    path: target.into(),
                }];
            }
            _ => {
                if let Some(text) = action.strip_prefix("book-text:") {
                    m.bookmark_input(text);
                } else if let Some(id) = action.strip_prefix("book-row:") {
                    let id: u64 = id.parse().map_err(|_| "ID favorito inválido")?;
                    click(
                        m,
                        |a| matches!(a,Action::Bookmark(_,B::Select(Row::Favourite(value)))if *value==id),
                    )?;
                } else if let Some(stamp) = action.strip_prefix("book-stale-remove:") {
                    let stamp: u64 = stamp.parse().map_err(|_| "Stamp inválido")?;
                    m.action(Action::Bookmark(stamp, B::Remove));
                } else if let Some(key) = action.strip_prefix("book-key:") {
                    use fltk::enums::Shortcut;
                    let (key, mods) = match key {
                        "space" => (Key::from_char(' '), Shortcut::None),
                        "insert" => (Key::Insert, Shortcut::None),
                        "marks-none" => (Key::from_char('d'), Shortcut::Ctrl),
                        "shiftup" => (Key::Up, Shortcut::Shift),
                        "shiftdown" => (Key::Down, Shortcut::Shift),
                        "shiftend" => (Key::End, Shortcut::Shift),
                        "up" => (Key::Up, Shortcut::None),
                        "down" => (Key::Down, Shortcut::None),
                        "home" => (Key::Home, Shortcut::None),
                        "end" => (Key::End, Shortcut::None),
                        "enter" => (Key::Enter, Shortcut::None),
                        "escape" => (Key::Escape, Shortcut::None),
                        "ctrla" => (Key::from_char('a'), Shortcut::Ctrl),
                        "ctrlf" => (Key::from_char('f'), Shortcut::Ctrl),
                        "delete" => (Key::Delete, Shortcut::None),
                        _ => return Err("Tecla favorito desconocida".into()),
                    };
                    crate::bookmarks_ui::key(m, key, mods, "", w, h);
                } else {
                    return Err("Comando favoritos desconocido".into());
                }
            }
        }
        return Ok(());
    }
    if action == "path-open-hit" {
        click(m, |a| matches!(a, Action::Omni))?;
        return Ok(());
    }
    if action == "view-menu-hit" {
        click(m, |a| matches!(a, Action::ViewMenu(_, _)))?;
        return Ok(());
    }
    if action == "path-accept-hit" {
        click(m, |a| matches!(a, Action::PathCommit))?;
        return Ok(());
    }
    if action == "path-cancel-hit" {
        click(m, |a| matches!(a, Action::PathCancel))?;
        return Ok(());
    }
    if action == "path-paste-prepare" {
        let context = m.text_context();
        if context.is_none() || m.text_paste.request(context).is_none() {
            return Err("Slot pegado no disponible".into());
        }
        return Ok(());
    }
    if action == "path-paste-deliver" {
        let accepted = m.receive_text_paste("changed");
        eprintln!("FILES_PATH_PASTE accepted={accepted}");
        return Ok(());
    }
    if let Some(text) = action.strip_prefix("path-text:") {
        crate::input(m, text);
        return Ok(());
    }
    if let Some(chord) = action.strip_prefix("path-key:") {
        let (modifier, name) = chord.split_once(':').ok_or("Atajo de ruta inválido")?;
        let (ctrl, shift) = match modifier {
            "plain" => (false, false),
            "shift" => (false, true),
            "ctrl" => (true, false),
            "ctrlshift" => (true, true),
            _ => return Err("Modificador inválido".into()),
        };
        let key = match name {
            "left" => Key::Left,
            "right" => Key::Right,
            "home" => Key::Home,
            "end" => Key::End,
            "backspace" => Key::BackSpace,
            "delete" => Key::Delete,
            "enter" => Key::Enter,
            "escape" => Key::Escape,
            "a" => Key::from_char('a'),
            "c" => Key::from_char('c'),
            "x" => Key::from_char('x'),
            "z" => Key::from_char('z'),
            "y" => Key::from_char('y'),
            _ => return Err("Tecla inválida".into()),
        };
        if let crate::path_edit::KeyResult::Copy(text) = m.path_key(key, ctrl, shift, false) {
            eprintln!("FILES_PATH_COPY bytes={}", text.len());
        }
        return Ok(());
    }
    for (prefix, drag, double) in [
        ("path-click:", false, false),
        ("path-drag:", true, false),
        ("path-double:", false, true),
    ] {
        if let Some(index) = action.strip_prefix(prefix) {
            let index = index.parse().map_err(|_| "Byte inválido")?;
            let (x, y) = crate::path_edit::painted_position(m, index)
                .ok_or("Cursor fixture fuera del campo visible")?;
            if !crate::path_edit::pointer(m, x, y, !drag, drag, false, double) {
                return Err("Gesto de edición rechazado".into());
            }
            return Ok(());
        }
    }
    if action == "path-release" {
        crate::path_edit::pointer(m, 0, 0, false, false, false, false);
        return Ok(());
    }
    if let Some(chord) = action.strip_prefix("nav:") {
        let (modifier, name) = chord.split_once(':').ok_or("Atajo fixture inválido")?;
        let (ctrl, shift) = match modifier {
            "plain" => (false, false),
            "shift" => (false, true),
            "ctrl" => (true, false),
            "ctrlshift" => (true, true),
            _ => return Err("Modificador inválido".into()),
        };
        let key = match name {
            "up" => Key::Up,
            "down" => Key::Down,
            "left" => Key::Left,
            "right" => Key::Right,
            "home" => Key::Home,
            "end" => Key::End,
            "page-up" => Key::PageUp,
            "page-down" => Key::PageDown,
            _ => return Err("Tecla inválida".into()),
        };
        crate::keyboard_nav::key(m, key, ctrl, shift, w, h);
        return Ok(());
    }
    if action.starts_with("clip-") {
        return crate::clipboard_scene::command(m, action, w, h);
    }
    if action.starts_with("dual-") {
        return crate::dual_scene::command(m, action, w, h);
    }
    if action.starts_with("columns-") {
        return crate::columns_scene::command(m, action, w, h);
    }
    if action == "bar-toggle" {
        crate::operation_bar::shortcut(m, Key::from_char('b'), true, true);
        return Ok(());
    }
    if let Some(index) = action.strip_prefix("bar-hit:") {
        let index: usize = index.parse().map_err(|_| "Índice de operación inválido")?;
        let area = m
            .hits
            .iter()
            .rev()
            .find(|h| matches!(h.action,Action::OperationBar(_,_,i) if i==index))
            .map(|h| h.rect)
            .ok_or("Operación no pintada")?;
        if !crate::operation_bar::pointer(m, area.x + area.w / 2, area.y + area.h / 2) {
            return Err("Botón no consumido".into());
        }
        return Ok(());
    }
    if action.starts_with("manager-") {
        use crate::size_panel::Command as C;
        match action {
            "manager-open" => click(m, |a| matches!(a, Action::Sizes))?,
            "manager-close" => click(m, |a| matches!(a, Action::SizePanel(_, C::Close)))?,
            "manager-marked-hit" => click(m, |a| matches!(a, Action::SizePanel(_, C::Marked)))?,
            "manager-blocks-hit" => click(m, |a| matches!(a, Action::SizePanel(_, C::Blocks(_))))?,
            "manager-blocks-key" => crate::size_panel::key(
                m,
                fltk::enums::Key::from_char('b'),
                fltk::enums::Shortcut::None,
            ),
            "manager-marked-blocks-key" => crate::size_panel::key(
                m,
                fltk::enums::Key::from_char('b'),
                fltk::enums::Shortcut::Shift,
            ),
            "manager-marked-key" => {
                crate::size_panel::key(m, Key::from_char('m'), fltk::enums::Shortcut::None)
            }
            "manager-report-marked-key" => {
                crate::size_panel::key(m, Key::from_char('i'), fltk::enums::Shortcut::Shift)
            }
            "manager-report-scope-hit" => {
                click(m, |a| matches!(a, Action::SizePanel(_, C::ReportScope(_))))?
            }
            "manager-order" => click(m, |a| matches!(a, Action::SizePanel(_, C::Order)))?,
            "manager-order-key" => {
                crate::size_panel::key(m, Key::from_char('s'), fltk::enums::Shortcut::None)
            }
            "manager-open-folder" => click(m, |a| matches!(a, Action::SizePanel(_, C::Open(_))))?,
            "manager-open-key" => crate::size_panel::key(m, Key::Enter, fltk::enums::Shortcut::Alt),
            "manager-info" => click(m, |a| matches!(a, Action::SizePanel(_, C::Details)))?,
            "manager-forget-hit" => click(m, |a| matches!(a, Action::SizePanel(_, C::Forget(_))))?,
            "manager-forget-report-hit" => click(m, |a| {
                matches!(a, Action::SizePanel(_, C::ForgetMeasured(_)))
            })?,
            "manager-forget-key" => {
                crate::size_panel::key(m, Key::from_char('o'), fltk::enums::Shortcut::None)
            }
            "manager-forget-review-key" => {
                crate::size_panel::key(m, Key::from_char('o'), fltk::enums::Shortcut::Shift)
            }
            "manager-report" => click(m, |a| matches!(a, Action::SizePanel(_, C::Report)))?,
            "manager-report-key" => {
                crate::size_panel::key(m, Key::from_char('i'), fltk::enums::Shortcut::None)
            }
            "manager-report-refresh" => {
                crate::size_panel::key(m, Key::F5, fltk::enums::Shortcut::None)
            }
            "manager-report-copy" => {
                click(m, |a| matches!(a, Action::SizePanel(_, C::CopyReport)))?
            }
            "manager-report-blocked-pause" => {
                crate::size_panel::key(m, Key::from_char('p'), fltk::enums::Shortcut::None)
            }
            "manager-cancel-hit" => click(m, |a| matches!(a, Action::SizePanel(_, C::Cancel(_))))?,
            "manager-recalculate-hit" => {
                click(m, |a| matches!(a, Action::SizePanel(_, C::Recalculate(_))))?
            }
            "manager-recalculate-paused" => {
                crate::size_panel::key(m, Key::F5, fltk::enums::Shortcut::None);
                crate::size_panel::key(m, Key::from_char('p'), fltk::enums::Shortcut::None);
            }
            "manager-recalculate-key" => {
                crate::size_panel::key(m, Key::F5, fltk::enums::Shortcut::None)
            }
            "manager-retry-hit" => click(m, |a| matches!(a, Action::SizePanel(_, C::Retry(_))))?,
            "manager-resume-hit" => {
                click(m, |a| matches!(a, Action::SizePanel(_, C::Pause(false))))?
            }
            "manager-stop-hit" => click(m, |a| matches!(a, Action::SizePanel(_, C::Stop)))?,
            "manager-recapture-hit" => {
                click(m, |a| matches!(a, Action::SizePanel(_, C::Recapture)))?
            }
            "manager-raw" => {
                let id = m
                    .sizes
                    .rows(&m.path)
                    .iter()
                    .find(|r| r.name.as_encoded_bytes().contains(&0xff))
                    .ok_or("Fila raw ausente")?
                    .id;
                click(
                    m,
                    |a| matches!(a,Action::SizePanel(_,C::Select(i))if *i==id),
                )?;
            }
            "manager-home" => crate::size_panel::key(m, Key::Home, fltk::enums::Shortcut::None),
            "manager-end" => crate::size_panel::key(m, Key::End, fltk::enums::Shortcut::None),
            "manager-down" => crate::size_panel::key(m, Key::Down, fltk::enums::Shortcut::None),
            "manager-escape" => crate::size_panel::key(m, Key::Escape, fltk::enums::Shortcut::None),
            "manager-refresh" => m.refresh_current(), // Private simulation of an async owned refresh.
            "manager-enqueue-paused" => {
                let stamp = m.size_panel.as_ref().ok_or("Gestor cerrado")?.stamp;
                m.action(Action::SizePanel(stamp, C::Listed));
                m.action(Action::SizePanel(stamp, C::Pause(true)));
            }
            _ => {
                if let Some(value) = action.strip_prefix("manager-old-forget:") {
                    let (stamp, id) = value.split_once(':').ok_or("sello/id")?;
                    m.action(Action::SizePanel(
                        stamp.parse().map_err(|_| "sello")?,
                        C::Forget(id.parse().map_err(|_| "id")?),
                    ));
                } else if let Some(id) = action.strip_prefix("manager-old-cancel:") {
                    let stamp = m.size_panel.as_ref().ok_or("Gestor cerrado")?.stamp;
                    m.action(Action::SizePanel(
                        stamp,
                        C::Cancel(id.parse().map_err(|_| "ID inválido")?),
                    ));
                } else if let Some(stamp) = action.strip_prefix("manager-old-stop:") {
                    m.action(Action::SizePanel(
                        stamp.parse().map_err(|_| "Sello inválido")?,
                        C::Stop,
                    ));
                } else {
                    return Err("Comando gestor desconocido".into());
                }
            }
        }
        return Ok(());
    }
    if let Some(text) = action.strip_prefix("filter-text:") {
        crate::input(m, text);
        return Ok(());
    }
    if action == "filter-list-backspace" {
        crate::keyboard_nav::backspace(m);
        return Ok(());
    }
    if action == "filter-focus" {
        m.action(Action::Filter);
        return Ok(());
    }
    if action == "filter-huge" {
        crate::input(m, &"x".repeat(1024 * 1024));
        return Ok(());
    }
    if let Some(name) = action.strip_prefix("filter-key:") {
        use fltk::enums::Shortcut;
        let (key, mods) = match name {
            "altleft" => (Key::Left, Shortcut::Alt),
            "altright" => (Key::Right, Shortcut::Alt),
            "altup" => (Key::Up, Shortcut::Alt),
            "left" => (Key::Left, Shortcut::None),
            "right" => (Key::Right, Shortcut::None),
            "home" => (Key::Home, Shortcut::None),
            "end" => (Key::End, Shortcut::None),
            "shiftleft" => (Key::Left, Shortcut::Shift),
            "shiftright" => (Key::Right, Shortcut::Shift),
            "backspace" => (Key::BackSpace, Shortcut::None),
            "delete" => (Key::Delete, Shortcut::None),
            "enter" => (Key::Enter, Shortcut::None),
            "tab" => (Key::Tab, Shortcut::None),
            "escape" => (Key::Escape, Shortcut::None),
            "space" => (Key::from_char(' '), Shortcut::None),
            "insert" => (Key::Insert, Shortcut::None),
            "marks-none" => (Key::from_char('d'), Shortcut::Ctrl),
            "shiftup" => (Key::Up, Shortcut::Shift),
            "shiftdown" => (Key::Down, Shortcut::Shift),
            "shiftend" => (Key::End, Shortcut::Shift),
            "up" => (Key::Up, Shortcut::None),
            "down" => (Key::Down, Shortcut::None),
            "undo" => (Key::from_char('z'), Shortcut::Ctrl),
            "redo" => (Key::from_char('z'), Shortcut::Ctrl | Shortcut::Shift),
            "all" => (Key::from_char('a'), Shortcut::Ctrl),
            "copy" => (Key::from_char('c'), Shortcut::Ctrl),
            "f2" => (Key::F2, Shortcut::None),
            "f5" => (Key::F5, Shortcut::None),
            "shiftf6" => (Key::F6, Shortcut::Shift),
            "ctrlf6" => (Key::F6, Shortcut::Ctrl),
            "f6" => (Key::F6, Shortcut::None),
            "f8" => (Key::F8, Shortcut::None),
            "page-next" => (Key::PageDown, Shortcut::Ctrl),
            "page-prev" => (Key::PageUp, Shortcut::Ctrl),
            "f9" => (Key::F9, Shortcut::None),
            "today" => (Key::from_char('1'), Shortcut::None),
            "yesterday" => (Key::from_char('2'), Shortcut::None),
            "seven-days" => (Key::from_char('3'), Shortcut::None),
            "last-hour" => (Key::from_char('4'), Shortcut::None),
            "last-24hours" => (Key::from_char('5'), Shortcut::None),
            "last-168hours" => (Key::from_char('6'), Shortcut::None),
            _ => return Err("Tecla de filtro desconocida".into()),
        };
        match m.filter_key(key, mods, "", 0) {
            crate::path_edit::KeyResult::Copy(text) => eprintln!(
                "FILES_FILTER_PRIVATE_COPY {}",
                serde_json::to_string(&text).unwrap()
            ),
            crate::path_edit::KeyResult::Unhandled => {
                crate::keyboard_nav::key(m, key, false, false, w, h);
            }
            crate::path_edit::KeyResult::Handled => {}
        }
        return Ok(());
    }
    for (prefix, drag, shift, double) in [
        ("filter-point:", false, false, false),
        ("filter-drag:", true, false, false),
        ("filter-shiftpoint:", false, true, false),
        ("filter-doublepoint:", false, false, true),
    ] {
        if let Some(byte) = action.strip_prefix(prefix) {
            let byte = byte
                .parse::<usize>()
                .map_err(|_| "Byte de filtro inválido")?;
            let (x, y) = m
                .filter_edit
                .geometry
                .painted_position(byte, m.filter_edit.epoch)
                .ok_or("Punto de filtro no pintado")?;
            if !crate::filter_edit::pointer(m, x, y, !drag, drag, shift, double) {
                return Err("Punto de filtro rechazado".into());
            }
            return Ok(());
        }
    }
    if let Some(query) = action.strip_prefix("filter-query:") {
        click(m, |a| matches!(a, Action::Filter))?;
        crate::input(m, query);
        return Ok(());
    }
    if action == "toggle-raw-name" {
        let i = m
            .entries
            .iter()
            .position(|e| e.name.as_encoded_bytes().contains(&0xff))
            .ok_or("raw name missing")?;
        m.select_entry(i, true, false);
        return Ok(());
    }
    if let Some(name) = action.strip_prefix("toggle-name:") {
        let i = index(m, name)?;
        m.select_entry(i, true, false);
        return Ok(());
    }
    if let Some(name) = action.strip_prefix("select-name:") {
        let i = index(m, name)?;
        m.select_entry(i, false, false);
        return Ok(());
    }
    if let Some(query) = action.strip_prefix("commands-query:") {
        let s = m.commands.as_mut().ok_or("Catálogo cerrado")?;
        s.select_input();
        s.input(query);
        return Ok(());
    }
    if let Some(id) = action.strip_prefix("commands-row:") {
        let s = m.commands.as_ref().ok_or("Catálogo cerrado")?;
        let row = s
            .matches
            .iter()
            .position(|&(i, _)| crate::command_palette::ITEMS[i].id == id)
            .ok_or("Comando no coincide")?;
        click(m, |a| matches!(a,Action::CommandSelect(_,i)if *i==row))?;
        return Ok(());
    }
    if let Some(name) = action.strip_prefix("commands-key:") {
        use fltk::enums::Shortcut;
        let key = match name {
            "enter" => Key::Enter,
            "escape" => Key::Escape,
            "down" => Key::Down,
            "up" => Key::Up,
            "home" => Key::Home,
            "end" => Key::End,
            "pageup" => Key::PageUp,
            "pagedown" => Key::PageDown,
            "backspace" => Key::BackSpace,
            _ => return Err("Tecla catálogo desconocida".into()),
        };
        crate::command_palette::key(m, key, Shortcut::None, "");
        return Ok(());
    }
    match action {
        "commands-shortcut" => {
            crate::command_palette::shortcut(
                m,
                Key::from_char('p'),
                fltk::enums::Shortcut::Ctrl | fltk::enums::Shortcut::Shift,
            );
            return Ok(());
        }
        "commands-menu-hit" => {
            click(m, |a| matches!(a, Action::Commands))?;
            return Ok(());
        }
        "commands-run-hit" => {
            click(m, |a| matches!(a, Action::CommandActivate(_)))?;
            return Ok(());
        }
        "commands-recapture-hit" => {
            click(m, |a| matches!(a, Action::CommandRecapture))?;
            return Ok(());
        }
        "commands-context-change" => {
            m.refresh_current();
            return Ok(());
        } // Own fixture simulates a background refresh.
        _ => {}
    }
    if action == "recursive-regex-hit" {
        click(m, |a| matches!(a, Action::SearchRegex))?;
        return Ok(());
    }
    if action == "recursive-content-hit" {
        click(m, |a| matches!(a, Action::SearchContent))?;
        return Ok(());
    }
    if let Some(text) = action.strip_prefix("recursive-content-input:") {
        m.search.input(text);
        return Ok(());
    }
    if let Some(query) = action.strip_prefix("recursive-query:") {
        m.search.focus(0);
        m.search.input(query);
        m.action(Action::SearchStart);
        return Ok(());
    }
    for (prefix, drag, shift, double) in [
        ("recursive-point:", false, false, false),
        ("recursive-drag:", true, false, false),
        ("recursive-shiftpoint:", false, true, false),
        ("recursive-doublepoint:", false, false, true),
    ] {
        if let Some(value) = action.strip_prefix(prefix) {
            let (field, byte) = value.split_once(':').ok_or("Punto de consulta inválido")?;
            let field = field.parse::<usize>().map_err(|_| "Campo inválido")?;
            let byte = byte.parse::<usize>().map_err(|_| "Byte inválido")?;
            let (x, y) = m
                .search
                .geometry
                .get(field)
                .and_then(|g| g.painted_position(byte, m.search.input_revision()))
                .ok_or("Punto de texto no pintado o fuera del campo")?;
            if !m.search.pointer(x, y, !drag, drag, shift, double) {
                return Err("Puntero no aceptado".into());
            }
            return Ok(());
        }
    }
    if let Some(name) = action.strip_prefix("recursive-key:") {
        use fltk::enums::Shortcut;
        let (key, modifiers) = match name {
            "altleft" => (Key::Left, Shortcut::Alt),
            "altright" => (Key::Right, Shortcut::Alt),
            "altup" => (Key::Up, Shortcut::Alt),
            "left" => (Key::Left, Shortcut::None),
            "right" => (Key::Right, Shortcut::None),
            "home" => (Key::Home, Shortcut::None),
            "end" => (Key::End, Shortcut::None),
            "shiftleft" => (Key::Left, Shortcut::Shift),
            "shiftright" => (Key::Right, Shortcut::Shift),
            "ctrlleft" => (Key::Left, Shortcut::Ctrl),
            "ctrlright" => (Key::Right, Shortcut::Ctrl),
            "delete" => (Key::Delete, Shortcut::None),
            "ctrlbackspace" => (Key::BackSpace, Shortcut::Ctrl),
            "undo" => (Key::from_char('z'), Shortcut::Ctrl),
            "redo" => (Key::from_char('z'), Shortcut::Ctrl | Shortcut::Shift),
            "space" => (Key::from_char(' '), Shortcut::None),
            "insert" => (Key::Insert, Shortcut::None),
            "marks-none" => (Key::from_char('d'), Shortcut::Ctrl),
            "shiftup" => (Key::Up, Shortcut::Shift),
            "shiftdown" => (Key::Down, Shortcut::Shift),
            "shiftend" => (Key::End, Shortcut::Shift),
            "up" => (Key::Up, Shortcut::None),
            "down" => (Key::Down, Shortcut::None),
            "f4" => (Key::F4, Shortcut::None),
            "f5" => (Key::F5, Shortcut::None),
            "shiftf6" => (Key::F6, Shortcut::Shift),
            "ctrlf6" => (Key::F6, Shortcut::Ctrl),
            "f6" => (Key::F6, Shortcut::None),
            "f8" => (Key::F8, Shortcut::None),
            "page-next" => (Key::PageDown, Shortcut::Ctrl),
            "page-prev" => (Key::PageUp, Shortcut::Ctrl),
            "f9" => (Key::F9, Shortcut::None),
            "today" => (Key::from_char('1'), Shortcut::None),
            "yesterday" => (Key::from_char('2'), Shortcut::None),
            "seven-days" => (Key::from_char('3'), Shortcut::None),
            "last-hour" => (Key::from_char('4'), Shortcut::None),
            "last-24hours" => (Key::from_char('5'), Shortcut::None),
            "last-168hours" => (Key::from_char('6'), Shortcut::None),
            "tab" => (Key::Tab, Shortcut::None),
            "backtab" => (Key::Tab, Shortcut::Shift),
            "enter" => (Key::Enter, Shortcut::None),
            "ctrlenter" => (Key::Enter, Shortcut::Ctrl),
            "escape" => (Key::Escape, Shortcut::None),
            "backspace" => (Key::BackSpace, Shortcut::None),
            "ctrlf" => (Key::from_char('f'), Shortcut::Ctrl),
            "ctrla" => (Key::from_char('a'), Shortcut::Ctrl),
            _ => return Err("Tecla de buscador desconocida".into()),
        };
        let before = (
            m.search.searching_current(),
            m.search.filters_open,
            m.search.report.as_ref().map_or(0, |r| r.hits.len()),
            m.search.input_revision(),
            m.search.report.is_some(),
        );
        crate::search_ui::key(m, key, modifiers, "", w, h);
        if name == "enter" {
            eprintln!(
                "FILES_SEARCH_ENTER current_before={} form_before={} hits_before={} input_changed={} current_after={} had_summary={}",
                before.0,
                before.1,
                before.2,
                before.3 != m.search.input_revision(),
                m.search.searching_current(),
                before.4
            );
        }
        return Ok(());
    }
    if let Some(index) = action.strip_prefix("recursive-field:") {
        let index = index.parse::<usize>().map_err(|_| "Campo inválido")?;
        click(m, |a| matches!(a, Action::SearchField(i) if *i == index))?;
        return Ok(());
    }
    if let Some(value) = action.strip_prefix("recursive-text:") {
        m.search.input(value);
        return Ok(());
    }
    if action == "recursive-order" {
        click(m, |a| matches!(a, Action::SearchOrder))?;
        return Ok(());
    }
    if action == "recursive-order-key" {
        crate::search_ui::key(m, Key::F7, fltk::enums::Shortcut::None, "", w, h);
        return Ok(());
    }
    if action == "recursive-filters" {
        click(m, |a| matches!(a, Action::SearchFilters))?;
        return Ok(());
    }
    if action == "recursive-pages" {
        click(m, |a| matches!(a, Action::SearchPages))?;
        return Ok(());
    }
    if action == "recursive-page-next" {
        click(m, |a| matches!(a, Action::SearchPageStep(1, _)))?;
        return Ok(());
    }
    if action == "recursive-page-prev" {
        click(m, |a| matches!(a, Action::SearchPageStep(-1, _)))?;
        return Ok(());
    }
    if let Some(epoch) = action.strip_prefix("recursive-page-old-next:") {
        m.action(Action::SearchPageStep(
            1,
            epoch.parse().map_err(|_| "Bad page epoch")?,
        ));
        return Ok(());
    }
    if action == "recursive-calendar" {
        click(m, |a| matches!(a, Action::SearchCalendar))?;
        return Ok(());
    }
    if let Some(name) = action.strip_prefix("recursive-preset:") {
        use files_core::search_dates::Preset;
        let preset = match name {
            "today" => Preset::Today,
            "yesterday" => Preset::Yesterday,
            "seven" => Preset::LastSevenDays,
            "hour" => Preset::LastHour,
            "24hours" => Preset::Last24Hours,
            "168hours" => Preset::Last168Hours,
            _ => return Err("Bad calendar preset".into()),
        };
        click(m, |a| matches!(a,Action::SearchDate(p)if *p==preset))?;
        return Ok(());
    }
    if action == "recursive-clear-filters" {
        click(m, |a| matches!(a, Action::SearchClearFilters))?;
        return Ok(());
    }
    if action == "recursive-start-hit" {
        click(m, |a| matches!(a, Action::SearchStart))?;
        return Ok(());
    }
    if let Some(index) = action.strip_prefix("recursive-mark:") {
        let index = index.parse().map_err(|_| "Índice inválido")?;
        m.action(Action::SearchToggle(index, m.search.result_epoch));
        return Ok(());
    }
    if action == "recursive-place-destination" {
        let path =
            std::env::var_os("FILES_TEST_BOOKMARK_PLACE").ok_or("Destino privado ausente")?;
        m.copy_form
            .as_mut()
            .ok_or("Sin destino")?
            .capture_destination(path.into());
        return Ok(());
    }
    if let Some(path) = action.strip_prefix("recursive-destination:") {
        let form = m.copy_form.as_mut().ok_or("Sin destino")?;
        form.select_all();
        form.input(path);
        return Ok(());
    }
    if let Some(value) = action.strip_prefix("recursive-old-check:") {
        let (epoch, index) = value.split_once(':').ok_or("Época/índice inválido")?;
        m.action(Action::SearchToggle(
            index.parse().map_err(|_| "Índice inválido")?,
            epoch.parse().map_err(|_| "Época inválida")?,
        ));
        return Ok(());
    }
    if let Some(index) = action.strip_prefix("recursive-check:") {
        let index = index.parse::<usize>().map_err(|_| "Índice inválido")?;
        let expected = m.search.result_epoch;
        click(
            m,
            |a| matches!(a,Action::SearchToggle(i,epoch)if *i==index&&*epoch==expected),
        )?;
        return Ok(());
    }
    if action == "recursive-marks-button" {
        click(m, |a| matches!(a, Action::SearchMarkAll(_)))?;
        return Ok(());
    }
    if let Some(index) = action.strip_prefix("recursive-select:") {
        m.action(Action::SearchSelect(
            index.parse().map_err(|_| "Índice inválido")?,
            m.search.result_epoch,
        ));
        return Ok(());
    }
    match action {
        "recursive-scope-menu" => click(m, |a| matches!(a, Action::SearchScopeMenu))?,
        "recursive-scope-parent" => click(m, |a| {
            matches!(
                a,
                Action::SearchScope(crate::search_state::Scope::Parent, _)
            )
        })?,
        "recursive-scope-result" => click(m, |a| {
            matches!(
                a,
                Action::SearchScope(crate::search_state::Scope::Result(_), _)
            )
        })?,
        "recursive-scope-back" => click(m, |a| {
            matches!(a, Action::SearchScope(crate::search_state::Scope::Back, _))
        })?,
        "prop-open" => m.action(Action::Properties),
        "prop-close" => click(m, |a| matches!(a, Action::PropertyClose))?,
        "prop-apply" => click(m, |a| matches!(a, Action::PropertyApply))?,
        "prop-toggle-focused" => {
            let focus = m
                .properties_dialog
                .as_ref()
                .ok_or("Propiedades cerradas")?
                .focus;
            click(m, |a| matches!(a,Action::PropertyBit(i)if *i==focus))?;
        }
        "prop-group-read-focus" => {
            for _ in 0..9 {
                if m.properties_dialog.as_ref().is_some_and(|d| d.focus == 3) {
                    break;
                }
                crate::properties_ui::key(m, Key::Tab, fltk::enums::Shortcut::None, h);
            }
        }
        "prop-readonly-attempt" => {
            m.action(Action::PropertyBit(0));
            m.action(Action::PropertyApply);
            if m.mutation_applied {
                return Err("Aplicación inesperada en objeto sólo lectura".into());
            }
        }
        "prop-home" => crate::properties_ui::key(m, Key::Home, fltk::enums::Shortcut::None, h),
        "prop-end" => crate::properties_ui::key(m, Key::End, fltk::enums::Shortcut::None, h),
        "prop-page-down" => {
            crate::properties_ui::key(m, Key::PageDown, fltk::enums::Shortcut::None, h)
        }
        "prop-last-bit" => crate::properties_ui::key(m, Key::Tab, fltk::enums::Shortcut::Shift, h),
        "recursive-open" => m.action(Action::Search),
        "recursive-close" => m.action(Action::SearchClose),
        "recursive-cancel" => m.action(Action::SearchCancel),
        "recursive-hidden" => m.action(Action::SearchHidden),
        "recursive-tab-last" => {
            let id = m.tabs.rows.last().ok_or("Sin pestañas")?.id;
            m.action(Action::Tab(crate::tabs::Command::Select(id)));
        }
        "recursive-tab-first" => {
            let id = m.tabs.rows.first().ok_or("Sin pestañas")?.id;
            m.action(Action::Tab(crate::tabs::Command::Select(id)));
        }
        "recursive-copy" => click(m, |a| matches!(a, Action::SearchTransfer(false)))?,
        "recursive-move" => click(m, |a| matches!(a, Action::SearchTransfer(true)))?,
        "recursive-background" => click(m, |a| matches!(a, Action::SearchBackgroundTab))?,
        "recursive-locate" => click(m, |a| matches!(a, Action::SearchOpen(true)))?,
        "recursive-enter" => click(m, |a| matches!(a, Action::SearchOpen(false)))?,
        "filter-clear-hit" => click(m, |a| matches!(a, Action::ClearFilter))?,
        "selection-menu" => click(m, |a| matches!(a, Action::SelectionMenu(_, _)))?,
        "selection-menu-filtered" | "selection-background-all" => {
            let desired = if action == "selection-menu-filtered" {
                crate::selection_tools::Command::Filtered
            } else {
                crate::selection_tools::Command::All
            };
            let i = m
                .context_menu
                .as_ref()
                .and_then(|menu| {
                    menu.items.iter().position(
                        |item| matches!(item.action,Some(Action::Selection(c))if c==desired),
                    )
                })
                .ok_or("Herramienta de selección no visible")?;
            click(m, |a| matches!(a, Action::ContextItem(j) if *j==i))?;
        }
        "selection-filtered" => {
            m.action(Action::Selection(crate::selection_tools::Command::Filtered))
        }
        "selection-all" => m.action(Action::Selection(crate::selection_tools::Command::All)),
        "selection-invert" => m.action(Action::Selection(crate::selection_tools::Command::Invert)),
        "selection-clear" => m.action(Action::Selection(crate::selection_tools::Command::Clear)),
        "mark-next" => m.mark_and_advance(),
        "size-hit" => click(m, |a| matches!(a, Action::CalculateSize))?,
        "sizes-listed-paused" => {
            m.action(Action::CalculateListedSizes);
            m.action(Action::PauseSizes(true));
        }
        "sizes-listed" => m.action(Action::CalculateListedSizes),
        "sizes-marked" => m.action(Action::CalculateMarkedSizes),
        "sizes-marked-allocated" => m.action(Action::CalculateMarkedAllocatedSizes),
        "sizes-pause" => m.action(Action::PauseSizes(true)),
        "sizes-resume" => m.action(Action::PauseSizes(false)),
        "sizes-retry" => m.action(Action::RetrySizeErrors),
        "sizes-stop" => m.action(Action::CancelListedSizes),
        "sizes-sort" => m.action(Action::Sort(files_core::listing::Sort::Size)),
        "sizes-refresh" => m.action(Action::Refresh),
        "size-key" => m.action(Action::CalculateSize),
        "sizes-selected-paused" => {
            m.action(Action::CalculateSize);
            m.action(Action::PauseSizes(true));
        }
        "thumb-select-red" | "thumb-select-video" | "thumb-select-raw" | "thumb-select-jpeg" => {
            let i = if action == "thumb-select-raw" {
                m.entries
                    .iter()
                    .position(|e| e.name.as_encoded_bytes().contains(&0xff))
                    .ok_or("raw fixture missing")?
            } else {
                index(
                    m,
                    match action {
                        "thumb-select-red" => "red.png",
                        "thumb-select-video" => "clip.mkv",
                        _ => "photo.jpg",
                    },
                )?
            };
            m.select_entry(i, false, false);
        }
        "thumb-trash" => m.action(Action::Trash),
        "thumb-assert-raw-space" => {
            let crate::text_preview_state::View::Image(ready) = m.preview.view() else {
                return Err("Imagen completa no lista".into());
            };
            if !m
                .preview
                .path()
                .is_some_and(|p| p.as_os_str().as_encoded_bytes().contains(&0xff))
                || ready.bitmap.source.is_none()
                || ready
                    .bitmap
                    .rgb
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .any(|p| *p != [180, 40, 200])
            {
                return Err("Origen raw o píxeles no coinciden".into());
            }
            eprintln!(
                "FILES_RAW_SPACE_PREVIEW_OK generation={} source={:?}",
                m.preview.revision(),
                ready.bitmap.source
            );
        }
        "thumb-preview" => m.action(Action::Preview),
        "thumb-click-preview" => click(m, |a| matches!(a, Action::Preview))?,
        "thumb-preview-close" => m.action(Action::ClosePreview),
        "thumb-callback" => {
            m.closing = true;
        }
        "history-back" => click(m, |a| matches!(a, Action::History(false)))?,
        "history-forward" => click(m, |a| matches!(a, Action::History(true)))?,
        "enter-one" | "enter-two" | "enter-deeper" | "enter-branch" => {
            let name = action.trim_start_matches("enter-");
            let i = index(m, name)?;
            let r = m
                .hits
                .iter()
                .find(|hit| matches!(hit.action,Action::Entry(j)if j==i))
                .map(|hit| hit.rect)
                .ok_or("Carpeta fuera de viewport")?;
            m.pointer_down(r.x + r.w / 2, r.y + r.h / 2, false, false, true);
        }
        "missing" => m.navigate(m.path.join("missing")),
        "omni" => m.action(Action::Omni),
        "paste-large" => crate::input(m, &"x".repeat(1024 * 1024)),
        "paste-small" => crate::input(m, "literal — á"),
        "omni-cancel" => m.end_path_edit(),
        "help" => click(m, |a| matches!(a, Action::Help))?,
        "help-key" => m.action(Action::Help),
        "help-page" => crate::help_view::key(m, Key::PageDown),
        "help-end" => crate::help_view::key(m, Key::End),
        "help-home" => crate::help_view::key(m, Key::Home),
        "help-close" => click(m, |a| matches!(a, Action::HelpClose))?,
        "help-escape" => crate::help_view::key(m, Key::Escape),
        "groups" => m.action(Action::View(View::Groups)),
        "list" => m.action(Action::View(View::List)),
        "grid" => click(m, |a| matches!(a, Action::View(View::Grid)))?,
        "spatial" | "columns" => m.action(Action::View(View::Columns)),
        "map" => m.action(Action::View(View::SpaceMap)),
        "grid-zoom-in" => click(m, |a| matches!(a,Action::GridZoom(f) if *f>1.))?,
        "grid-zoom-out" => click(m, |a| matches!(a,Action::GridZoom(f) if *f<1.))?,
        "grid-last" => crate::grid_view::move_focus(m, i32::MAX, false, w, h),
        "grid-first" => crate::grid_view::move_focus(m, i32::MIN, false, w, h),
        "grid-range" => {
            let cols = crate::grid_view::Metrics::new(
                crate::grid_view::width(m, w),
                m.grid_zoom,
                m.order.len(),
            )
            .columns;
            crate::grid_view::move_focus(m, cols as i32, true, w, h);
        }
        "grid-context" | "grid-open-folder" => {
            let hit=m.hits.iter().find(|hit|matches!(hit.action,Action::Entry(i) if action=="grid-context" || m.entries[i].is_dir())).ok_or("Target grilla ausente")?;
            let r = hit.rect;
            if action == "grid-context" {
                context_menu::open_at(m, r.x + r.w / 2, r.y + r.h / 2, w, h);
            } else {
                m.pointer_down(r.x + r.w / 2, r.y + r.h / 2, false, false, true);
            }
        }
        "back" => {
            let path = m.path.parent().ok_or("Sin padre")?.to_path_buf();
            m.action(Action::Navigate(path));
        }
        "full" => m.action(Action::WindowMode(
            files_core::theme::WindowMode::Fullscreen,
        )),
        "normal" => m.action(Action::WindowMode(files_core::theme::WindowMode::Normal)),
        "duplicates-open" => m.action(Action::Duplicates),
        "duplicates-menu-hit" => click(m, |a| matches!(a, Action::Duplicates))?,
        "checksums-menu-hit" => click(m, |a| matches!(a, Action::Checksums))?,
        "checksums-toggle-focused-mark" => {
            let i = m.selected.ok_or("Sin archivo enfocado")?;
            m.select_entry(i, true, false);
        }
        "checksums-open" => m.action(Action::Checksums),
        "checksums-start-hit" => click(m, |a| {
            matches!(a, Action::Checksum(_, crate::checksum_ui::Command::Start))
        })?,
        "checksums-cancel-hit" => click(m, |a| {
            matches!(a, Action::Checksum(_, crate::checksum_ui::Command::Cancel))
        })?,
        "checksums-close-hit" => click(m, |a| {
            matches!(a, Action::Checksum(_, crate::checksum_ui::Command::Close))
        })?,
        "checksums-capture-hit" => click(m, |a| {
            matches!(
                a,
                Action::Checksum(_, crate::checksum_ui::Command::Recapture)
            )
        })?,
        "checksums-copy-hit" => click(m, |a| {
            matches!(a, Action::Checksum(_, crate::checksum_ui::Command::Copy))
        })?,
        "duplicates-next-hit" => click(m, |a| {
            matches!(a, Action::Checksum(_, crate::checksum_ui::Command::Page(1)))
        })?,
        "duplicates-prev-hit" => click(m, |a| {
            matches!(
                a,
                Action::Checksum(_, crate::checksum_ui::Command::Page(-1))
            )
        })?,
        "duplicates-group-next-hit" => click(m, |a| {
            matches!(
                a,
                Action::Checksum(_, crate::checksum_ui::Command::Group(1))
            )
        })?,
        "duplicates-group-prev-hit" => click(m, |a| {
            matches!(
                a,
                Action::Checksum(_, crate::checksum_ui::Command::Group(-1))
            )
        })?,
        "duplicates-locate-hit" => click(m, |a| {
            matches!(a, Action::Checksum(_, crate::checksum_ui::Command::Locate))
        })?,
        "checksums-last" | "checksums-first" => {
            let stamp = m.checksums.as_ref().ok_or("SHA-256 cerrado")?.stamp;
            m.action(Action::Checksum(
                stamp,
                crate::checksum_ui::Command::Move(if action == "checksums-first" {
                    i32::MIN
                } else {
                    i32::MAX
                }),
            ));
        }
        "folder-save-hit" => click(m, |a| matches!(a, Action::SaveFolderView))?,
        "folder-forget-hit" => click(m, |a| matches!(a, Action::ForgetFolderView))?,
        "folder-cancel-close" => m.action(Action::CancelClose),
        "folder-discard-close" => m.action(Action::DiscardClose),
        "preferences" => click(m, |a| matches!(a, Action::Settings))?,
        "toggle-hit" => click(m, |a| matches!(a, Action::ToggleHidden))?,
        "toggle-shortcut" => m.action(Action::ToggleHidden),
        "menus-close" => {
            m.settings = false;
            m.actions_menu = false;
            context_menu::dismiss(m);
        }
        "inspector-close" => {
            if m.inspector {
                m.action(Action::Inspector);
            }
        }
        "select-batch" => {
            let a = index(m, "README.md")?;
            let b = index(m, "video.mp4")?;
            m.select_entry(a, false, false);
            m.select_entry(b, true, false);
        }
        "select-hidden" => {
            let a = index(m, ".secreto")?;
            m.select_entry(a, false, false);
        }
        "right-selected" | "right-outside" => {
            let i = index(
                m,
                if action == "right-selected" {
                    "README.md"
                } else {
                    "música.flac"
                },
            )?;
            let r = m
                .hits
                .iter()
                .find(|hit| {
                    matches!(hit.action,Action::Entry(j) if j==i) && hit.generation == m.generation
                })
                .map(|hit| hit.rect)
                .ok_or("Entrada sin hit visible")?;
            if !context_menu::open_at(m, r.x + r.w / 2, r.y + r.h / 2, w, h) {
                return Err("Menú rechazado".into());
            }
        }
        "terminal-hit" => click(m, |a| matches!(a, Action::Terminal))?,
        "terminal-context" => {
            let i = m
                .context_menu
                .as_ref()
                .ok_or("Sin menú")?
                .items
                .iter()
                .position(|i| matches!(i.action, Some(Action::Terminal)))
                .ok_or("Sin terminal")?;
            click(m, |a| matches!(a,Action::ContextItem(j)if *j==i))?;
        }
        "right-background" => {
            if !context_menu::open_at(m, w / 2, h - 70, w, h) {
                return Err("Fondo sin menú".into());
            }
        }
        "menu-key" => {
            if !context_menu::open_keyboard(m, w, h) {
                return Err("Menú teclado rechazado".into());
            }
        }
        "escape" => context_menu::key(m, Key::Escape),
        "outside" => context_menu::pointer(m, 1, 1, true),
        // By action, not position: the menus have section titles now.
        "menu-rename" | "menu-create" => {
            let i = m
                .context_menu
                .as_ref()
                .ok_or("Sin menú")?
                .items
                .iter()
                .position(|item| match action {
                    "menu-rename" => matches!(item.action, Some(Action::Rename)),
                    _ => matches!(item.action, Some(Action::NewEntry(false))),
                })
                .ok_or("Entrada de menú no visible")?;
            click(m, |a| matches!(a, Action::ContextItem(j) if *j == i))?;
        }
        "dialog-cancel" => m.action(Action::RenameCancel),
        "refresh" => m.action(Action::Refresh),
        "tip-advance" => {
            let now = std::time::Instant::now();
            m.status_tips.set_hover(false);
            m.status_tips.tick(now, false);
            m.status_tips.tick(now, true);
            m.status_tips
                .tick(now + std::time::Duration::from_secs(3), true);
        }
        _ => return Err("Comando fixture ocultos desconocido".into()),
    }
    Ok(())
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn marker(m: &Model, seq: u64, w: i32, h: i32, label: &str) -> Option<String> {
    if !m.scene_ready() && (m.error.is_empty() || !m.loaded) {
        return None;
    }
    let map = if m.view == View::SpaceMap {
        let line = crate::space_map_scene::marker(m, seq)?;
        Some(
            serde_json::from_str::<serde_json::Value>(line.strip_prefix("FILES_SPACE_STATE ")?)
                .ok()?,
        )
    } else {
        None
    };
    let name = |i: usize| m.entries.get(i).map(|e| hex(e.name.as_encoded_bytes()));
    let mut value = serde_json::json!({"seq":seq,"view":match m.view { View::Groups=>"groups",View::List=>"list",View::Grid=>"grid",View::Spatial=>"spatial",View::Columns=>"columns",View::SpaceMap=>"space_map" },"show_hidden":m.show_hidden,"order":m.order.iter().filter_map(|&i|name(i)).collect::<Vec<_>>(),"selection":m.selection.iter().filter_map(|&i|name(i)).collect::<Vec<_>>(),"selected":m.selected.and_then(name),"entry_hits":m.hits.iter().filter_map(|hit|if let Action::Entry(i)=hit.action{name(i)}else{None}).collect::<Vec<_>>(),"context":m.context_menu.as_ref().map(|menu|serde_json::json!({"target":format!("{:?}",menu.target),"items":menu.items.iter().map(|item|item.label.as_ref()).collect::<Vec<_>>(),"rect":[menu.rect.x,menu.rect.y,menu.rect.w,menu.rect.h]})),"preferences":m.settings,"dialog":m.rename_dialog.as_ref().map(|d|match d.mode {crate::rename_dialog::Mode::Rename=>"rename",crate::rename_dialog::Mode::File=>"file",crate::rename_dialog::Mode::Folder=>"folder"}),"config_dirty":m.config_dirty,"native_frame_label":label,"width":w,"height":h,"map":map,"tip_offset":m.status_tips.offset(),"tip":m.status_tips.text(),"error":m.error,"path":m.path.to_str(),"path_raw":m.path.as_os_str().as_encoded_bytes(),"scroll":m.scroll,"visible":m.visible.iter().filter_map(|&i|name(i)).collect::<Vec<_>>(),"help":m.help.as_ref().map(|help|serde_json::json!({"top":help.top,"page":help.page})),"history_back":m.history.can_back(),"history_forward":m.history.can_forward(),"editing":m.editing,"edit":m.edit,"replace_edit":m.replace_edit,"generation":m.generation,"grid_zoom":m.grid_zoom,"grid_columns":crate::grid_view::Metrics::new(crate::grid_view::width(m,w),m.grid_zoom,m.order.len()).columns});
    let extra = serde_json::json!({"tabs":m.tabs.rows.iter().map(|t|serde_json::json!({"id":t.id,"path_raw":t.path.as_os_str().as_encoded_bytes()})).collect::<Vec<_>>(),"tab_active":m.tabs.active,"tab_closed":m.tabs.closed.len(),"tab_stored_bytes":m.tabs.stored_bytes(),"anchor":m.anchor.and_then(name),"descending":m.descending,"query":m.query,"pan_x":m.pan_x,"preview":match m.preview.view(){crate::text_preview_state::View::Closed=>"closed",crate::text_preview_state::View::Loading=>"loading",crate::text_preview_state::View::Ready(_)=>"text",_=>"other"},"preview_drained":m.preview.is_drained(),"selected_metadata":m.selected.and_then(|i|m.entries.get(i)).is_some_and(|e|e.metadata.is_some()),"calculate_targets":m.hits.iter().filter(|h|matches!(h.action,Action::CalculateSize)).map(|h|[h.rect.x,h.rect.y,h.rect.w,h.rect.h]).collect::<Vec<_>>()});
    if let serde_json::Value::Object(extra) = extra {
        value.as_object_mut().expect("state object").extend(extra);
    }
    value.as_object_mut().expect("state object").insert(
        "destination_queue_idle".into(),
        serde_json::json!(!m.copy_busy()),
    );
    value.as_object_mut().expect("state object").insert("sftp_transport".into(), serde_json::json!({"busy":m.sftp.transport.busy(),"phase":m.sftp.transport.status.phase,"point":m.sftp.transport.status.point.as_ref().map(|p|p.as_os_str().as_encoded_bytes()),"diagnostic":m.sftp.transport.status.diagnostic,"open":m.sftp.transport.status.open,"writable":m.sftp.transport.status.writable}));
    value.as_object_mut().expect("state object").insert("search_pages".into(),serde_json::json!({"mode":m.search.page_mode,"current":m.search.pages.current.map(|p|p.sequence),"oldest":m.search.pages.oldest(),"latest":m.search.pages.latest(),"found":m.search.pages.found,"loading":m.search.pages.loading,"ended":m.search.pages.ended,"complete":m.search.pages.complete,"cancelled":m.search.pages.cancelled,"bytes":m.search.pages.bytes(&m.search.report),"epoch":m.search.result_epoch,"next_hit":m.hits.iter().any(|h|matches!(h.action,Action::SearchPageStep(1,e)if e==m.search.result_epoch)),"prev_hit":m.hits.iter().any(|h|matches!(h.action,Action::SearchPageStep(-1,e)if e==m.search.result_epoch)),"start_hit":m.hits.iter().any(|h|matches!(h.action,Action::SearchPages)),"rects":m.hits.iter().filter(|h|matches!(h.action,Action::SearchPages|Action::SearchPageStep(..))).map(|h|[h.rect.x,h.rect.y,h.rect.w,h.rect.h]).collect::<Vec<_>>()}));
    value.as_object_mut().expect("state object").insert("sftp_profiles".into(), serde_json::json!({"loaded":m.sftp_profiles.loaded,"busy":m.sftp_profiles.busy(),"request":m.sftp_profiles.request_id(),"error":m.sftp_profiles.error,"notice":m.sftp_profiles.notice,"unsynced":m.sftp_profiles.unsynced,"rows":m.sftp_profiles.document.rows.iter().map(|e|serde_json::json!({"id":e.id,"label":e.label,"fields":e.criteria.fields})).collect::<Vec<_>>()}));
    value
        .as_object_mut()
        .expect("state object")
        .insert("sftp_report".into(), crate::sftp_report_ui::fixture(m));
    value.as_object_mut().expect("state object").insert("sftp".into(),serde_json::json!(m.sftp.form.as_ref().map(|f|serde_json::json!({"stamp":f.stamp,"epoch":f.edit_epoch,"profiles":f.profiles,"profile_input_painted":f.profile_geometry.position(0,f.edit_epoch).is_some(),"profile_label":f.profile_label,"profile_selected":f.profile_selected,"fields":f.fields,"focus":f.focus,"error":f.error,"review":f.review.as_ref().map(|r|&r.source),"capability_done":f.capability.is_some(),"capability_error":f.capability.as_ref().and_then(|v|v.as_ref().err()),"summary":f.capability.as_ref().and_then(|v|v.as_ref().ok()).map(|c|c.summary()),"sshfs":f.capability.as_ref().and_then(|v|v.as_ref().ok()).map(|c|c.sshfs.is_some()),"tools_ready":f.capability.as_ref().and_then(|v|v.as_ref().ok()).map(|c|c.tools_ready()),"active_probe":m.sftp.active,"hit_count":m.hits.iter().filter(|h|matches!(h.action,Action::SftpCommand(..))).count()}))));
    value.as_object_mut().expect("state object").insert("destination_editor".into(),serde_json::json!(m.copy_form.as_ref().map(|d|serde_json::json!({"exdev":d.prepared.as_ref().is_some_and(|p|p.context().is_some()),"moving":d.moving,"multi_parents":d.multi.as_ref().map(|p|p.groups().len()),"multi_roots":d.multi.as_ref().map(|_|d.names.len()),"text":d.destination,"epoch":d.editor.epoch,"caret":d.editor.cursor.caret,"anchor":d.editor.cursor.anchor,"selection":d.selected_text(),"preparing":d.preparing,"prepared":d.prepared.is_some(),"captured":d.captured_destination.is_some(),"skip_existing":d.existing_destination==files_core::copy_engine::ExistingDestination::Skip,"conflict_policy":format!("{:?}",d.existing_destination),"options_open":d.options_open,"policy_controls":m.hits.iter().filter(|h|matches!(h.action,Action::CopyOptions|Action::CopyVisiblePublication|Action::CopySymlinkTimes)).count(),"publication":format!("{:?}",d.publication),"preserve_symlink_times":d.preserve_symlink_times,"name_focus":d.name_focus,"name_row":d.name_field.row,"name_draft":d.name_field.draft,"name_epoch":d.name_field.editor.epoch,"name_original_label":d.name_field.original_label(),"name_original":d.names.get(d.name_field.row).map(|n|n.as_os_str().as_encoded_bytes()),"names_modified":d.name_field.plan.modified(),"names_changed":d.name_field.changed_count(),"error":d.error}))));
    value.as_object_mut().expect("state object").insert("copy_progress".into(),serde_json::json!({"queued":m.copy.queued_count(),"phase":format!("{:?}",m.copy.phase()),"active":m.copy.active_id(),"entries":m.copy.progress().map(|p|p.entries_copied),"bytes":m.copy.progress().map(|p|p.bytes_copied)}));
    value.as_object_mut().expect("state object").insert("notice_owner".into(),serde_json::json!({"copy":m.copy_notice_id,"allowed":m.notice_copy_outcome().map(|o|o.id),"detail_targets":m.hits.iter().filter(|h|matches!(h.action,Action::CopyFullReport)).count()}));
    value.as_object_mut().expect("state object").insert("copy_result".into(),serde_json::json!(m.copy_last.as_ref().map(|o|serde_json::json!({"id":o.id,"ok":o.result.is_ok(),"exdev":o.exdev.is_some(),"moving":o.moving,"batch_states":match &o.result{Err(crate::copy_job::Failure::Batch(b))=>Some(b.ledger.states().iter().map(|s|format!("{s:?}")).collect::<Vec<_>>()),_=>None},"completed":o.report().map(|r|r.completed_count),"publication":o.report().map(|r|format!("{:?}",r.publication)),"visible_started":o.report().map(|r|r.visible_started_count),"visible_completed":o.report().map(|r|r.visible_completed_count),"visible_uncertain":o.report().map(|r|r.visible_uncertain_count),"symlink_times_omitted":o.report().map(|r|r.symlink_times_omitted),"cancelled":o.cancelled(),"cause":o.engine_error().map(|e|e.cause.to_string()),"renamed":o.report().map(|r|r.renamed_count),"completed_paths":o.report().map(|r|r.completed.iter().map(|p|p.as_os_str().as_encoded_bytes()).collect::<Vec<_>>()),"skipped":o.report().map(|r|r.skipped_count),"skipped_paths":o.report().map(|r|r.skipped.iter().map(|p|p.as_os_str().as_encoded_bytes()).collect::<Vec<_>>()),"notice":m.notice_open,"message":m.operation_message}))));
    value
        .as_object_mut()
        .expect("state object")
        .insert("group_ends".into(), serde_json::json!(m.group_ends));
    value.as_object_mut().expect("state object").insert(
        "size_sort".into(),
        serde_json::json!({"column":format!("{:?}",m.sort),"descending":m.descending}),
    );
    value.as_object_mut().expect("state object").insert("direct_summaries".into(), serde_json::json!(m.summaries.iter().filter_map(|(i,s)|m.entries.get(*i).map(|e|serde_json::json!({"name":e.name.as_encoded_bytes(),"entries":s.entries,"status":format!("{:?}",s.status),"label":s.label(),"display":m.sizes.detail(&m.path,&e.name).unwrap_or_else(||s.label())}))).collect::<Vec<_>>()));
    value.as_object_mut().expect("state object").insert("listing_watch".into(),serde_json::json!({"settled":m.listing_sync.settled(),"pending":m.listing_sync.pending(),"notice":m.listing_sync.notice,"refreshes":m.listing_sync.refreshes,"stopped":m.listing_sync.stopped()}));
    value.as_object_mut().expect("state object").insert("folder_usage".into(), serde_json::json!({"text": m.du_text, "running": m.du_job.is_some(), "pending":m.du_requested.is_some(), "target":m.du_target.as_ref().map(|p|p.as_os_str().as_encoded_bytes()),"queue_work":m.sizes.has_work(),"paused":m.sizes.paused(),"summary":m.sizes.summary(),"rows":m.entries.iter().filter(|e|e.is_dir()).filter_map(|e|m.sizes.label(&m.path,&e.name).map(|label|serde_json::json!({"name":e.name.as_encoded_bytes(),"label":label,"detail":m.sizes.detail(&m.path,&e.name)}))).collect::<Vec<_>>()}));
    value
        .as_object_mut()
        .expect("state object")
        .insert("commands".into(), crate::command_palette::fixture_state(m));
    value.as_object_mut().expect("state object").insert(
        "size_manager".into(),
        crate::size_panel::fixture_state(m).unwrap_or(serde_json::Value::Null),
    );
    value.as_object_mut().expect("state object").insert("folder_views".into(), serde_json::json!({
        "loaded":m.folder_views.loaded,"busy":m.folder_views.busy(),
        "unresolved":m.folder_views.needs_resolution(),"error":m.folder_views.error,
        "notice":m.folder_views.notice,"closing":m.closing,"restoring":m.folder_restore.is_some(),
        "rows":m.folder_views.document.rows.iter().map(|e|serde_json::json!({
            "path_raw":e.path.as_os_str().as_encoded_bytes(),"view":e.settings.view,
            "sort":e.settings.sort,"descending":e.settings.descending,
            "zoom":e.settings.zoom,"scroll":e.settings.scroll
        })).collect::<Vec<_>>()
    }));
    value
        .as_object_mut()
        .expect("state object")
        .insert("checksums".into(), crate::checksum_ui::fixture(m));
    value
        .as_object_mut()
        .expect("state object")
        .insert("bookmarks".into(), crate::bookmarks_ui::fixture(m));
    value
        .as_object_mut()
        .expect("state object")
        .insert("properties".into(), crate::properties_dialog::fixture(m));
    value.as_object_mut().expect("state object").insert("filter_editor".into(),serde_json::json!({"active":m.filter_edit.active,"epoch":m.filter_edit.epoch,"caret":m.filter_edit.cursor.caret,"anchor":m.filter_edit.cursor.anchor,"selected_text":&m.query[m.filter_edit.cursor.range(&m.query)]}));
    value
        .as_object_mut()
        .expect("state object")
        .insert("operation_bar".into(), crate::operation_bar::fixture(m));
    value.as_object_mut().expect("state object").insert(
        "launcher".into(),
        serde_json::json!(m.open_job.as_ref().map(|job| format!("{:?}", job.kind()))),
    );
    value.as_object_mut().expect("state object").insert("search".into(),serde_json::json!({"capture_busy":m.search_transfer.busy(),"capture_id":m.search_transfer.id(),"transfer_controls":m.hits.iter().filter(|h|matches!(h.action,Action::SearchTransfer(_))).map(|h|[h.rect.x,h.rect.y,h.rect.w,h.rect.h]).collect::<Vec<_>>(),"visible":m.search.visible,"root":m.search.root.as_os_str().as_encoded_bytes(),"query":m.search.query,"content":m.search.content,"content_mode":m.search.content_mode,"editing_input":m.search.editing_input,"caret":m.search.editors[m.search.field].caret,"anchor":m.search.editors[m.search.field].anchor,"selected_text":m.search.selected_text(),"fields":m.search.fields,"field":m.search.field,"filters_open":m.search.filters_open,"calendar_open":m.search.calendar_open,"date_busy":m.search.date_busy(),"calendar_controls":m.hits.iter().filter(|h|matches!(h.action,Action::SearchDate(_))).count(),"date_range":m.search.date_range.as_ref().map(|r|serde_json::json!({"after":r.after,"before":r.before,"reference":r.reference,"start":r.start,"end":r.end,"preset":format!("{:?}",r.preset)})),"hidden":m.search.hidden,"running":m.search.running(),"provisional_snippets":m.search.provisional.as_ref().map(|hits|hits.iter().map(|h|&h.snippet).collect::<Vec<_>>()),"provisional":m.search.provisional.as_ref().map(|hits|hits.iter().map(|h|h.relative.as_os_str().as_encoded_bytes()).collect::<Vec<_>>()),"progress":m.search.progress,"status":m.search.status,"error":m.search.error,"selected":m.search.selected,"display_order":m.search.order.order.label(),"display_paths":m.search.order.visible(m.search.hits()).iter().map(|&i|m.search.hits()[i].relative.as_os_str().as_encoded_bytes()).collect::<Vec<_>>(),"scroll":m.search.scroll,"marked":m.search.marks.paths().map(|p|p.as_os_str().as_encoded_bytes()).collect::<Vec<_>>(),"marks_explicit":m.search.marks.explicit,"marks_pruned":m.search.marks.pruned,"result_epoch":m.search.result_epoch,"background_count":m.search.background_count,"background_tab":m.search.background_tab,"background_hit":m.hits.iter().any(|h|matches!(h.action,Action::SearchBackgroundTab)),"result_controls":m.hits.iter().filter(|h|matches!(h.action,Action::SearchOpen(_)|Action::SearchBackgroundTab)).map(|h|[h.rect.x,h.rect.y,h.rect.w,h.rect.h]).collect::<Vec<_>>(),"row_hits":m.hits.iter().filter(|h|matches!(h.action,Action::SearchSelect(..))).count(),"report":m.search.report}));
    if let Some(q) = value
        .get_mut("search")
        .and_then(serde_json::Value::as_object_mut)
    {
        q.insert(
            "content_regex".into(),
            serde_json::json!(m.search.content_regex),
        );
        q.insert(
            "regex_rects".into(),
            serde_json::json!(
                m.hits
                    .iter()
                    .filter(|h| matches!(h.action, Action::SearchRegex))
                    .map(|h| [h.rect.x, h.rect.y, h.rect.w, h.rect.h])
                    .collect::<Vec<_>>()
            ),
        );
        q.insert(
            "regex_peer_rects".into(),
            serde_json::json!(
                m.hits
                    .iter()
                    .filter(|h| matches!(
                        h.action,
                        Action::SearchScopeMenu
                            | Action::SearchPages
                            | Action::SearchPageStep(..)
                            | Action::SavedSearches(..)
                    ))
                    .map(|h| [h.rect.x, h.rect.y, h.rect.w, h.rect.h])
                    .collect::<Vec<_>>()
            ),
        );
        q.insert("scope_menu".into(), serde_json::json!(m.search.scope_menu));
        q.insert(
            "scope_controls".into(),
            serde_json::json!(
                m.hits
                    .iter()
                    .filter(|h| matches!(h.action, Action::SearchScope(..)))
                    .count()
            ),
        );
        q.insert(
            "scope_root_painted".into(),
            serde_json::json!(
                m.hits
                    .iter()
                    .any(|h| matches!(h.action, Action::SearchScopeMenu))
            ),
        );
        q.insert(
            "filter_targets".into(),
            serde_json::json!(
                m.hits
                    .iter()
                    .filter(|h| matches!(h.action, Action::SearchField(1..=4)))
                    .map(|h| [h.rect.x, h.rect.y, h.rect.w, h.rect.h])
                    .collect::<Vec<_>>()
            ),
        );
        q.insert(
            "header_painted".into(),
            serde_json::json!(
                m.hits.iter().any(
                    |h| h.generation == m.generation && matches!(h.action, Action::SearchClose)
                )
            ),
        );
        q.insert(
            "input_painted".into(),
            serde_json::json!(
                m.search.geometry[m.search.primary()]
                    .position(0, m.search.input_revision())
                    .is_some()
            ),
        );
        q.insert(
            "criteria_painted".into(),
            serde_json::json!((1..=4).all(|i| {
                m.search.geometry[i]
                    .position(0, m.search.input_revision())
                    .is_some()
            })),
        );
    }
    value.as_object_mut().expect("state object").insert("path_editor".into(),serde_json::json!({"caret":m.path_edit.cursor.caret,"anchor":m.path_edit.cursor.anchor,"revision":m.path_edit.cursor.revision,"scroll":m.path_edit.scroll,"selected":hex(m.edit[m.path_edit.cursor.range(&m.edit)].as_bytes()),"dragging":m.path_edit.dragging}));
    value.as_object_mut().expect("state object").insert("notice_view".into(),serde_json::json!({
        "open":m.notice_open,"scroll":m.notice_scroll,"rows":m.notice_rows,"total":m.operation_details.len(),
        "pan":m.notice_pan,"pan_limit":m.notice_pan_limit,
        "navigation":m.hits.iter().filter_map(|h|if let Action::NoticeNavigate(c)=h.action{Some(format!("{c:?}"))}else{None}).collect::<Vec<_>>()
    }));
    value
        .as_object_mut()
        .expect("state object")
        .insert("saved_searches".into(), crate::saved_search_ui::snapshot(m));
    let session = serde_json::json!({"session_mode":match m.session.mode {crate::session_state::Mode::Disabled=>"disabled",crate::session_state::Mode::Off=>"off",crate::session_state::Mode::Opening=>"opening",crate::session_state::Mode::Writable=>"writable",crate::session_state::Mode::ReadOnly=>"readonly"},"session_pending":m.session.pending.is_some(),"session_in_flight":m.session.in_flight,"session_error":m.session.error,"session_notice":m.session.notice});
    if let serde_json::Value::Object(extra) = session {
        value.as_object_mut().expect("state object").extend(extra);
    }
    value
        .as_object_mut()
        .expect("state object")
        .insert("columns".into(), crate::columns_scene::marker(m));
    value
        .as_object_mut()
        .expect("state object")
        .insert("dual".into(), crate::dual_scene::marker(m));
    value.as_object_mut().expect("state object").insert("clipboard".into(),serde_json::json!({"count":m.clipboard_state.count(),"generation":m.clipboard_state.generation(),"kind":m.clipboard_state.kind().map(|k|format!("{k:?}")),"busy":m.clipboard_state.busy(),"error":m.clipboard_state.error,"text_pending":m.text_paste.pending(),"actions_menu":m.actions_menu,"action_targets":m.hits.iter().filter(|h|matches!(h.action,Action::ClipboardCopy|Action::ClipboardCut|Action::ClipboardPaste|Action::ContextItem(_))).map(|h|serde_json::json!({"action":match h.action {Action::ContextItem(i)=>format!("context:{i}"),Action::ClipboardCopy=>"copy".into(),Action::ClipboardCut=>"cut".into(),_=>"paste".into()},"rect":[h.rect.x,h.rect.y,h.rect.w,h.rect.h]})).collect::<Vec<_>>() }));
    value.as_object_mut().expect("state object").insert(
        "native_transfer".into(),
        serde_json::json!({"message":m.operation_message,"drained":m.native_transfer.is_drained(),"fixture_ack":m.native_transfer.fixture_ack(),"hover_path":m.native_transfer.hover().map(|t|t.path.as_os_str().as_encoded_bytes())}),
    );
    Some(format!("FILES_HIDDEN_STATE {value}"))
}
