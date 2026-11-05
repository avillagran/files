//! Fixture actions exercise the same state, pointer and clipboard handlers as UI.
use crate::{
    model::{Action, Model},
    painter::FltkPainter,
    text_preview_state::View,
};
fn open(m: &mut Model, name: &str) -> Result<(), String> {
    let index = m
        .entries
        .iter()
        .position(|e| e.name == name)
        .ok_or("preview fixture missing entry")?;
    m.select_entry(index, false, false);
    m.open_preview();
    Ok(())
}
fn follow_named(m: &mut Model, needle: &str) -> Result<(), String> {
    let View::Markdown(ready) = m.preview.view_mut() else {
        return Err("Markdown no está listo".into());
    };
    let offset = ready
        .rendered
        .document
        .text()
        .find(needle)
        .ok_or("Enlace de fixture no encontrado")?;
    ready.rendered.document.move_to(offset, false);
    ready.rendered.reveal();
    let point = ready
        .rendered
        .point_for_offset(&mut FltkPainter, offset + 2);
    if !m.follow_preview_link(point.0, point.1) {
        return Err("Hit del enlace fixture falló".into());
    }
    Ok(())
}
/// Nautilus-floor fixture commands (Shift+Delete, trash, links, menus,
/// subfolder refresh); same Actions as the keyboard/mouse handlers.
fn floor_command(m: &mut Model, action: &str) -> Option<Result<(), String>> {
    use crate::trash_browser::Command as T;
    if let Some(i) = action.strip_prefix("nf-ow-select:") {
        let i = i.parse().unwrap_or(0);
        m.action(Action::OpenWithCmd(crate::open_with_ui::Command::Select(i)));
        return Some(Ok(()));
    }
    if let Some(spec) = action.strip_prefix("nf-marquee:") {
        let v: Vec<i32> = spec.split(',').filter_map(|n| n.parse().ok()).collect();
        if v.len() != 5 {
            return Some(Err("nf-marquee: x0,y0,x1,y1,additive".into()));
        }
        m.pointer_down(v[0], v[1], v[4] == 1, false, false);
        m.pointer_drag((v[0] + v[2]) / 2, (v[1] + v[3]) / 2);
        m.pointer_drag(v[2], v[3]);
        m.pointer_up();
        let mut names: Vec<String> = m
            .selection
            .iter()
            .filter_map(|i| m.entries.get(*i))
            .map(|e| e.name.to_string_lossy().into_owned())
            .collect();
        names.sort();
        eprintln!("FILES_NF_SELECTION {}", names.join("|"));
        return Some(Ok(()));
    }
    if let Some(spec) = action.strip_prefix("nf-device:") {
        let (node, op) = spec.rsplit_once(':').unwrap_or((spec, ""));
        let op = match op {
            "mount" => Some(files_core::devices::Op::Mount),
            "unmount" => Some(files_core::devices::Op::Unmount),
            "eject" => Some(files_core::devices::Op::Eject),
            _ => None,
        };
        m.action(Action::Device(node.into(), op));
        return Some(Ok(()));
    }
    if let Some(i) = action.strip_prefix("nf-template:") {
        let list = files_core::quick_ops::templates(&files_core::quick_ops::templates_dir());
        let Some(t) = i.parse::<usize>().ok().and_then(|i| list.get(i).cloned()) else {
            return Some(Err(format!(
                "nf-template: sin plantilla {i} ({})",
                list.len()
            )));
        };
        m.action(Action::NewFromTemplate(t));
        return Some(Ok(()));
    }
    if let Some(glob) = action.strip_prefix("nf-select-pattern:") {
        m.action(Action::SelectPattern);
        if let Err(e) = m.filter_input(glob) {
            return Some(Err(e.into()));
        }
        m.filter_key(fltk::enums::Key::Enter, fltk::enums::Shortcut::None, "", 0);
        return Some(Ok(()));
    }
    if action == "nf-copy" {
        m.action(Action::ClipboardCopy);
        return Some(Ok(()));
    }
    if action == "nf-link-copied" {
        m.action(Action::LinkCopied);
        return Some(Ok(()));
    }
    if let Some(name) = action.strip_prefix("nf-preview:") {
        return Some(open(m, name));
    }
    if action == "nf-preview-copy-all" {
        m.action(Action::SelectPreview);
        m.action(Action::CopyPreview);
        let n = m.preview.document().map_or(0, |d| d.selected_text().len());
        eprintln!("FILES_PREVIEW_COPY selected_bytes={n}");
        return Some(Ok(()));
    }
    if action == "nf-folder-with" {
        m.action(Action::FolderWithSelection);
        return Some(Ok(()));
    }
    if action == "nf-sort-type" {
        m.action(Action::Sort(files_core::listing::Sort::Type));
        return Some(Ok(()));
    }
    if action == "nf-order" {
        let names: Vec<String> = m
            .order
            .iter()
            .filter_map(|i| m.entries.get(*i))
            .map(|e| e.name.to_string_lossy().into_owned())
            .collect();
        eprintln!("FILES_NF_ORDER {}", names.join("|"));
        return Some(Ok(()));
    }
    if action == "nf-rail" {
        m.rail = true;
        return Some(Ok(()));
    }
    if action == "nf-bookmark-add" {
        m.action(Action::BookmarkAdd);
        return Some(Ok(()));
    }
    if action == "nf-devices" {
        let list: Vec<String> = m
            .devices
            .list
            .iter()
            .map(|d| {
                format!(
                    "{}={}@{}",
                    d.node.display(),
                    d.title(),
                    d.mount
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default()
                )
            })
            .collect();
        let rail: Vec<String> = m
            .hits
            .iter()
            .filter(|h| h.generation == m.generation && h.rect.x < 170)
            .filter_map(|h| match &h.action {
                Action::Device(n, op) => Some(format!("device:{}:{:?}", n.display(), op)),
                Action::Navigate(p) => Some(format!("nav:{}", p.display())),
                _ => None,
            })
            .collect();
        eprintln!(
            "FILES_NF_DEVICES path={} busy={} message={:?} list={} rail={}",
            m.path.display(),
            m.devices.busy(),
            m.operation_message,
            list.join(";"),
            rail.join(";")
        );
        return Some(Ok(()));
    }
    if action == "nf-hits" {
        let rows: Vec<String> = m
            .hits
            .iter()
            .filter(|h| h.generation == m.generation)
            .filter_map(|h| match h.action {
                Action::Entry(i) => m.entries.get(i).map(|e| {
                    format!(
                        "{}@{},{},{},{}",
                        e.name.to_string_lossy(),
                        h.rect.x,
                        h.rect.y,
                        h.rect.w,
                        h.rect.h
                    )
                }),
                _ => None,
            })
            .collect();
        let l = m.listing_rect;
        eprintln!(
            "FILES_NF_HITS listing={},{},{},{} {}",
            l.x,
            l.y,
            l.w,
            l.h,
            rows.join(";")
        );
        return Some(Ok(()));
    }
    if let Some(name) = action.strip_prefix("nf-select:") {
        let names: Vec<&str> = name.split('|').collect();
        m.selection.clear();
        for (k, n) in names.iter().enumerate() {
            let Some(i) = m.entries.iter().position(|e| e.name == *n) else {
                return Some(Err(format!("nf-select: falta {n}")));
            };
            m.select_entry(i, k > 0, false);
        }
        return Some(Ok(()));
    }
    let trash = |m: &mut Model, c: T| {
        let id = m
            .trash_browser
            .as_ref()
            .map(|p| p.id)
            .ok_or("papelera cerrada")?;
        m.trash_browser_command(id, c);
        Ok(())
    };
    Some(match action {
        "nf-delete" => {
            m.action(Action::DeletePermanently);
            eprintln!("FILES_NF_CONFIRM open={}", m.delete.confirm.is_some());
            Ok(())
        }
        "nf-confirm" => {
            m.action(Action::DeleteAnswer(true));
            Ok(())
        }
        "nf-cancel" => {
            m.action(Action::DeleteAnswer(false));
            Ok(())
        }
        "nf-links" => {
            m.action(Action::CreateLinks);
            Ok(())
        }
        "nf-trash" => {
            m.action(Action::Trash);
            Ok(())
        }
        "nf-open-trash" => {
            m.action(Action::OpenTrash);
            Ok(())
        }
        "nf-trash-restore" => trash(m, T::Restore),
        "nf-trash-purge" => trash(m, T::Purge),
        "nf-trash-empty" => trash(m, T::Empty),
        "nf-trash-close" => trash(m, T::Close),
        "nf-trash-state" => {
            let p = m.trash_browser.as_ref();
            eprintln!(
                "FILES_NF_TRASH open={} ready={} count={} notice={:?} busy={}",
                p.is_some(),
                p.is_some_and(|p| p.ready()),
                p.map_or(0, |p| p.catalog.items.len()),
                m.delete.trash_notice,
                m.delete.busy()
            );
            Ok(())
        }
        "nf-context" => {
            crate::context_menu::open_keyboard(m, 1000, 650);
            let labels: Vec<String> = m
                .context_menu
                .as_ref()
                .map(|c| {
                    c.items
                        .iter()
                        .map(|i| format!("{}{}", if i.section { "#" } else { "" }, i.label.trim()))
                        .collect()
                })
                .unwrap_or_default();
            eprintln!("FILES_NF_CONTEXT {}", labels.join(" | "));
            Ok(())
        }
        "nf-open" => {
            m.action(Action::Open);
            Ok(())
        }
        "nf-open-with" => {
            m.action(Action::OpenWith);
            Ok(())
        }
        "nf-ow-open" => {
            m.action(Action::OpenWithCmd(crate::open_with_ui::Command::Open));
            Ok(())
        }
        "nf-ow-default" => {
            m.action(Action::OpenWithCmd(
                crate::open_with_ui::Command::SetDefault,
            ));
            Ok(())
        }
        "nf-ow-all" => {
            m.action(Action::OpenWithCmd(crate::open_with_ui::Command::ToggleAll));
            Ok(())
        }
        "nf-ow-state" => {
            let p = m.open_with.as_ref();
            eprintln!(
                "FILES_NF_OPEN_WITH open={} mime={:?} default={:?} all={} selected={} rows={} notice={:?} job={} error={:?}",
                p.is_some(),
                p.and_then(|p| p.mime()),
                p.and_then(|p| p.default_id()),
                p.is_some_and(|p| p.show_all),
                p.map_or(0, |p| p.selected),
                p.map(|p| p.row_ids().join(",")).unwrap_or_default(),
                p.and_then(|p| p.notice.clone()),
                m.open_job.is_some(),
                m.error
            );
            Ok(())
        }
        "nf-context-close" => {
            m.context_menu = None;
            Ok(())
        }
        "nf-state" => {
            let rows: Vec<String> = m
                .entries
                .iter()
                .enumerate()
                .map(|(i, e)| {
                    format!(
                        "{}={}",
                        e.name.to_string_lossy(),
                        m.summaries.get(&i).map(|s| s.label()).unwrap_or_default()
                    )
                })
                .collect();
            eprintln!(
                "FILES_NF_STATE delete_busy={} message={:?} rows={}",
                m.delete.busy(),
                m.operation_message,
                rows.join(";")
            );
            Ok(())
        }
        _ => return None,
    })
}
pub fn command(m: &mut Model, action: &str) -> Result<(), String> {
    if let Some(result) = floor_command(m, action) {
        return result;
    }
    match action {
        "open" => open(m, "A.txt")?,
        "office" => open(m, "report.docx")?,
        "office-bad" => open(m, "bad.docx")?,
        "office-oversize" => open(m, "too-large.docx")?,
        "office-dtd" => open(m, "dtd.docx")?,
        "office-symlink" => open(m, "link.docx")?,
        "office-truncated" => open(m, "truncated.docx")?,
        "office-raw" => {
            #[cfg(unix)]
            let name = b"raw-\xff.docx".as_slice();
            #[cfg(not(unix))]
            let name = b"raw.docx".as_slice();
            let i = m
                .entries
                .iter()
                .position(|e| e.name.as_encoded_bytes() == name)
                .ok_or("Fixture DOCX raw ausente")?;
            m.select_entry(i, false, false);
            m.open_preview();
        }
        "office-rapid" => {
            open(m, "report.docx")?;
            open(m, "bad.docx")?;
            open(m, "report.docx")?;
        }
        "office-assert-a" | "office-assert-b" | "office-assert-raw" => {
            let View::Ready(r) = m.preview.view() else {
                return Err("DOCX no listo".into());
            };
            let expected = if action == "office-assert-a" {
                "ALFA"
            } else if action == "office-assert-b" {
                "BETA"
            } else {
                "RAW"
            };
            let text = r.document.text();
            if r.encoding != "DOCX · texto extraído"
                || r.source.is_none()
                || !text.contains(expected)
                || !text.contains("niño 文档 🙂\tfin\n")
                || text.contains("BORRADO")
                || text.contains("IGNORAR")
                || text.contains("ALFA") && expected != "ALFA"
            {
                return Err("Texto DOCX/origen efectivo inesperado".into());
            }
            eprintln!(
                "FILES_OFFICE_TEXT_OK expected={} source={:?}",
                expected, r.source
            );
        }
        "office-select" => {
            m.action(Action::SelectPreview);
            m.action(Action::CopyPreview);
            let d = m.preview.document().ok_or("DOCX no listo")?;
            if d.selected_text() != d.text() || d.text().is_empty() {
                return Err("Selección DOCX incompleta".into());
            }
            eprintln!(
                "FILES_OFFICE_SELECTION_OK bytes={} clipboard_handler_only=true",
                d.text().len()
            );
        }
        "rotation-open" => open(m, "quadrants.png")?,
        "rotation-right" | "rotation-left" => {
            let delta = if action == "rotation-right" { 1 } else { -1 };
            let hit = m
                .hits
                .iter()
                .find(|h| {
                    h.generation == m.generation
                        && matches!(h.action, Action::PreviewRotate(d) if d==delta)
                })
                .ok_or("Botón de giro no dibujado")?;
            if hit.action.accessible_name().is_none() {
                return Err("Giro sin nombre accesible".into());
            }
            let action = hit.action.clone();
            m.action(action);
        }
        "rotation-reset" => m.action(Action::PreviewImageReset),
        "rotation-state" => {
            let viewport = m.preview.bitmap_viewport_mut().ok_or("No hay bitmap")?;
            let body = viewport.body;
            eprintln!(
                "FILES_PREVIEW_ROTATION turns={} zoom={} body={},{},{},{}",
                viewport.quarter_turns(),
                viewport.zoom.max(1.),
                body.x,
                body.y,
                body.w,
                body.h
            );
        }
        "neighbor-first" => open(m, "a.png")?,
        "neighbor-next" | "neighbor-back" => {
            let delta = if action == "neighbor-next" { 1 } else { -1 };
            let hit = m
                .hits
                .iter()
                .find(|h| {
                    h.generation == m.generation
                        && matches!(h.action, Action::PreviewNeighbor(d) if d == delta)
                })
                .ok_or("Botón de navegación de preview no dibujado")?;
            if hit.rect.w < 32 || hit.rect.h < 28 || hit.action.accessible_name().is_none() {
                return Err("Botón sin target/nombre accesible".into());
            }
            let action = hit.action.clone();
            m.action(action);
        }
        "neighbor-rapid" => {
            m.action(Action::PreviewNeighbor(-1));
            m.action(Action::PreviewNeighbor(1));
            m.action(Action::PreviewNeighbor(-1));
        }
        "neighbor-assert-red" | "neighbor-assert-blue" | "neighbor-assert-raw-green" => {
            let View::Image(r) = m.preview.view() else {
                return Err("Imagen no lista".into());
            };
            let selected = m
                .selected
                .and_then(|i| m.entries.get(i))
                .ok_or("Sin selección")?;
            let raw = action == "neighbor-assert-raw-green";
            let expected = if raw {
                std::ffi::OsStr::new("c-invalid.png").to_os_string()
            } else if action.ends_with("red") {
                "a.png".into()
            } else {
                "b.webp".into()
            };
            #[cfg(unix)]
            let expected = if raw {
                use std::os::unix::ffi::OsStringExt;
                std::ffi::OsString::from_vec(b"c-\xff.png".to_vec())
            } else {
                expected
            };
            let colour = if raw {
                [20, 220, 40]
            } else if action.ends_with("red") {
                [220, 20, 40]
            } else {
                [20, 40, 220]
            };
            if selected.name != expected
                || m.preview.path() != Some(m.path.join(&expected).as_path())
                || r.bitmap.source.is_none()
                || !r
                    .bitmap
                    .rgb
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .all(|p| p.iter().zip(colour).all(|(a, b)| a.abs_diff(b) < 8))
            {
                return Err("Selección/ruta raw/FD/píxeles no corresponden al destino".into());
            }
            if m.order.len() != 4
                || m.order
                    .iter()
                    .any(|i| m.entries[*i].name.as_encoded_bytes().starts_with(b"."))
            {
                return Err("Ocultos alteraron el recorrido visible".into());
            }
            eprintln!(
                "FILES_PREVIEW_NEIGHBOR_OK name={:?} raw={} source={:?}",
                selected.name, raw, r.bitmap.source
            );
        }

        "archive" => open(m, "catalog.zip")?,
        "archive-empty" => open(m, "empty.zip")?,
        "archive-raw" => open(m, "raw.zip")?,
        "archive-large" => open(m, "large.zip")?,
        "archive-bad" => open(m, "bad.zip")?,
        "archive-over" => open(m, "over.zip")?,
        "archive-assert" => {
            let doc = m.preview.document().ok_or("ZIP no listo")?;
            for needle in [
                "ZIP · listado",
                "7 entradas",
                "carpeta/niño.txt",
                "RUTA INSEGURA",
                "NOMBRE DUPLICADO",
                "[enlace",
                "CRC NO comprobados",
                "Enumeradas 7 de 7",
            ] {
                if !doc.text().contains(needle) {
                    return Err(format!("Falta dato ZIP {needle}"));
                }
            }
            eprintln!("FILES_ZIP_CATALOG_OK count=7 metadata_only=true extraction=false");
        }
        "archive-assert-raw" => {
            let doc = m.preview.document().ok_or("ZIP no listo")?;
            if !doc.text().contains("raw\\xFF.bin") {
                return Err("Nombre raw no preservado".into());
            }
            eprintln!("FILES_ZIP_RAW_NAME_OK escaped=true");
        }
        "archive-assert-truncated" => {
            let View::Ready(r) = m.preview.view() else {
                return Err("ZIP no listo".into());
            };
            if !r.truncated
                || !r.document.text().contains("LISTADO TRUNCADO")
                || r.document.text().len() > 128 * 1024
            {
                return Err("Listado no está limitado".into());
            }
            eprintln!(
                "FILES_ZIP_TRUNCATED_OK bytes={} lines={}",
                r.document.text().len(),
                r.document.line_count()
            );
        }
        "archive-rapid" => {
            open(m, "large.zip")?;
            open(m, "raw.zip")?;
            open(m, "catalog.zip")?;
        }
        "close-archive-pending" => {
            open(m, "large.zip")?;
            m.preview.pump();
            m.preview.close();
            m.closing = true;
        }
        "folder" => open(m, "folder")?,
        "folder-replaced" => open(m, "replaced")?,
        "folder-empty" => open(m, "empty")?,
        "folder-assert" => {
            if !m.preview.is_directory() {
                return Err("Preview no usa carpeta".into());
            }
            let doc = m.preview.document().ok_or("Carpeta no lista")?;
            if !doc.text().contains("8 bytes")
                || !doc.text().contains("Entradas descendientes: 5")
                || !doc.text().contains("No sigue enlaces")
            {
                return Err("Resumen de carpeta incorrecto".into());
            }
            eprintln!("FILES_FOLDER_SUMMARY_OK bytes=8 entries=5 symlink_followed=false");
        }
        "folder-assert-empty" => {
            let doc = m.preview.document().ok_or("Carpeta no lista")?;
            if !doc.text().contains("0 bytes") || !doc.text().contains("Entradas descendientes: 0")
            {
                return Err("Carpeta vacía incorrecta".into());
            }
            eprintln!("FILES_FOLDER_EMPTY_OK bytes=0 entries=0");
        }
        "folder-rapid" => {
            open(m, "folder")?;
            open(m, "empty")?;
            open(m, "folder")?;
        }
        "close-folder-pending" => {
            open(m, "folder")?;
            m.preview.pump();
            m.preview.close();
            m.closing = true;
        }
        "audio" => open(m, "song.flac")?,
        "audio-wav" => open(m, "sound.wav")?,
        "audio-long" => open(m, "long.flac")?,
        "audio-bad" => open(m, "bad.flac")?,
        "audio-symlink" => open(m, "symlink.flac")?,
        "audio-assert" => {
            let Some(document) = m.preview.document() else {
                return Err("Audio no listo".into());
            };
            let text = document.text();
            for needle in [
                "Audio · metadatos",
                "Reproducción a demanda",
                "Canción 漢 á",
                "Autora",
                "Álbum",
                "48000 Hz",
                "Canales: 1",
            ] {
                if !text.contains(needle) {
                    return Err(format!("Falta metadato {needle}"));
                }
            }
            eprintln!("FILES_AUDIO_METADATA_OK unicode=true content=actual-tags");
        }
        "audio-play" | "audio-pause" => m.action(Action::PreviewAudioToggle),
        "audio-seek" => m.action(Action::PreviewAudioSeek(1.5)),
        "audio-rewind" => m.preview.seek_audio_to(0.0),
        "audio-end" => m.preview.seek_audio_to(f64::MAX),
        "audio-timeline" => {
            let View::Ready(ready) = m.preview.view() else {
                return Err("Audio no listo".into());
            };
            let timeline = ready.audio.as_ref().ok_or("Audio sin controles")?.timeline;
            if timeline.w <= 0 {
                return Err("Timeline aún no pintada".into());
            }
            let selected = ready.document.selection_range();
            m.preview_pointer_down(
                timeline.x + timeline.w * 3 / 4,
                timeline.y + timeline.h / 2,
                false,
            );
            if m.preview.document().map(|d| d.selection_range()) != Some(selected) {
                return Err("Timeline robó selección del documento".into());
            }
        }
        "audio-assert-playing" | "audio-assert-paused" => {
            if m.preview.audio_info().is_none()
                || (action == "audio-assert-playing" && !m.preview.audio_running())
                || (action == "audio-assert-paused"
                    && (m.preview.audio_playing()
                        || !m.preview.is_drained()
                        || m.preview.audio_notice().is_some()))
            {
                return Err("Estado de reproducción de audio no corresponde".into());
            }
            eprintln!(
                "FILES_AUDIO_TRANSPORT_OK {}",
                serde_json::json!({"requested":m.preview.audio_playing(),"running":m.preview.audio_running(),"position":m.preview.audio_position(),"drained":m.preview.is_drained(),"notice":m.preview.audio_notice()})
            );
        }
        "audio-source-empty" => {
            if m.preview.document().is_some() {
                return Err("Se conservaron metadatos del original eliminado".into());
            }
            eprintln!("FILES_AUDIO_SOURCE_EMPTY");
        }
        "audio-source-a" | "audio-source-b" | "audio-source-c" => {
            let View::Ready(ready) = m.preview.view() else {
                return Err("Audio no listo".into());
            };
            let title = match action {
                "audio-source-a" => "Canción 漢 á",
                "audio-source-b" => "Canción 漢 ó",
                _ => "Canción 漢 é",
            };
            if ready.source.is_none() || !ready.document.text().contains(title) {
                return Err("Los metadatos no corresponden al origen verificado".into());
            }
            eprintln!(
                "FILES_AUDIO_SOURCE_OK {}",
                serde_json::json!({"title":title,"source":ready.source,"revision":m.preview.revision()})
            );
        }
        "audio-select" => {
            let wanted = "Canción 漢 á";
            let doc = m.preview.document_mut().ok_or("Audio no listo")?;
            let begin = doc.text().find(wanted).ok_or("Título no encontrado")?;
            doc.select(begin, begin + wanted.len());
            if doc.selected_text() != wanted {
                return Err("Selección metadatos distinta".into());
            }
            eprintln!("FILES_AUDIO_SELECTION_OK utf8=true clipboard=not-validated");
        }
        "audio-assert-truncated" => {
            let View::Ready(r) = m.preview.view() else {
                return Err("Audio no listo".into());
            };
            if !r.truncated
                || !r.document.text().contains("Etiquetas recortadas")
                || r.document.text().len() > 16384
            {
                return Err("Límite tags no aplicado".into());
            }
            eprintln!("FILES_AUDIO_TRUNCATED_OK bytes={}", r.document.text().len());
        }
        "audio-rapid" => {
            open(m, "sound.wav")?;
            open(m, "long.flac")?;
            open(m, "song.flac")?;
        }
        "close-audio-pending" => {
            open(m, "song.flac")?;
            m.preview.pump();
            m.preview.close();
            m.closing = true;
        }
        "pdf" => open(m, "pages.pdf")?,
        "pdf-text" => m.action(Action::PreviewPdfText),
        "pdf-text-select" => {
            let doc = m.preview.document().ok_or("Texto PDF no listo")?;
            let wanted = format!("PDF pagina {}", m.preview.pdf_page());
            let start = doc.text().find(&wanted).ok_or("Falta texto extraído PDF")?;
            let a = m
                .text_viewport
                .point_for_offset(&mut FltkPainter, doc, start);
            let b = m
                .text_viewport
                .point_for_offset(&mut FltkPainter, doc, start + wanted.len());
            m.preview_pointer_down(a.0, a.1, false);
            m.preview_pointer_drag(b.0, b.1);
            m.text_viewport.dragging = false;
            if m.preview
                .document()
                .ok_or("PDF desapareció")?
                .selected_text()
                != wanted
            {
                return Err("Selección texto PDF no coincide".into());
            }
            m.action(Action::CopyPreview);
            eprintln!(
                "FILES_PDF_TEXT_SELECTION_OK page={} copy_handler=invoked clipboard=not-validated",
                m.preview.pdf_page()
            );
        }
        "pdf-next" => m.action(Action::PreviewPdfPage(1)),
        "pdf-prev" => m.action(Action::PreviewPdfPage(-1)),
        "pdf-rapid" => {
            m.preview.change_pdf_page(-1);
            m.preview.change_pdf_page(1);
        }
        "pdf-zoom" => m.action(Action::PreviewImageZoom(2.0)),
        "pdf-pan" => {
            let View::Pdf(r) = m.preview.view() else {
                return Err("PDF no listo".into());
            };
            let body = r.viewport.body;
            let (x, y) = (body.x + body.w / 2, body.y + body.h / 2);
            m.preview_pointer_down(x, y, false);
            m.preview_pointer_drag(x - 80, y - 60);
            if let Some(viewport) = m.preview.bitmap_viewport_mut() {
                viewport.drag = None;
            }
        }
        "pdf-assert-pan" => {
            let View::Pdf(r) = m.preview.view() else {
                return Err("PDF no listo".into());
            };
            if r.viewport.zoom < 2.0 || r.viewport.pan_y <= 0.0 {
                return Err("Zoom/arrastre PDF no aplicado".into());
            }
            eprintln!(
                "FILES_PDF_ZOOM_PAN_OK zoom={} pan={},{}",
                r.viewport.zoom, r.viewport.pan_x, r.viewport.pan_y
            );
        }
        "pdf-assert-unavailable" => {
            if !matches!(m.preview.view(), View::Loading | View::Error(_)) {
                return Err("PDF obsoleto aún visible".into());
            }
            eprintln!("FILES_PDF_UNAVAILABLE_OK");
        }
        "pdf-bad" => open(m, "bad.pdf")?,
        "pdf-symlink" => open(m, "symlink.pdf")?,
        "pdf-assert-first" | "pdf-assert-second" | "pdf-assert-blue-first" => {
            let View::Pdf(r) = m.preview.view() else {
                return Err("PDF no listo".into());
            };
            let page = if action == "pdf-assert-second" { 2 } else { 1 };
            let blue = page == 2 || action == "pdf-assert-blue-first";
            let count = r
                .frame
                .bitmap
                .rgb
                .as_chunks::<3>()
                .0
                .iter()
                .filter(|p| {
                    if !blue {
                        p[0] > 180 && p[2] < 70
                    } else {
                        p[2] > 180 && p[0] < 70
                    }
                })
                .count();
            if r.frame.page != page
                || r.frame.pages != 2
                || count * 10 < r.frame.bitmap.rgb.len() / 3 * 8
            {
                return Err("Página PDF/píxeles no corresponden".into());
            }
            eprintln!(
                "FILES_PDF_PAGE_OK page={} pages={} bytes={} pan={},{}",
                page,
                r.frame.pages,
                r.frame.bitmap.rgb.len(),
                r.viewport.pan_x,
                r.viewport.pan_y
            );
        }
        "close-pdf-pending" => {
            m.preview.change_pdf_page(-1);
            m.preview.pump();
            m.preview.close();
            m.closing = true;
        }
        "gif-thumbnails" => {
            eprintln!("FILES_GIF_THUMBNAILS {}", m.thumbnails.snapshot());
        }
        "gif" => open(m, "clip.gif")?,
        "video" => open(m, "clip.mkv")?,
        "preview-close" => m.action(Action::ClosePreview),
        "video-audio" => m.action(Action::PreviewVideoAudio),
        "video-assert-audio-tracks" => {
            let View::Video(r) = m.preview.view() else {
                return Err("Vídeo no listo".into());
            };
            if r.frame.video_index != 1 || r.frame.audio.as_ref().map(|a| a.index) != Some(0) {
                return Err("Pistas de fixture no corresponden a sus índices efectivos".into());
            }
            eprintln!(
                "FILES_VIDEO_AUDIO_TRACKS_OK video=1 audio=0 enabled={}",
                m.preview.video_audio()
            );
        }
        "video-assert-backend-notice" => {
            let View::Video(r) = m.preview.view() else {
                return Err("Se perdieron controles/último frame tras fallo de salida".into());
            };
            if m.preview.video_playing() || r.notice.is_none() || r.frame.bitmap.source.is_none() {
                return Err("Fallo backend no dejó diagnóstico/reintento seguro".into());
            }
            eprintln!("FILES_VIDEO_BACKEND_NOTICE_OK source_verified=true retry_muted=true");
        }
        "video-zoom" => m.action(Action::PreviewImageZoom(2.)),
        "video-pan" => {
            let View::Video(r) = m.preview.view() else {
                return Err("Vídeo no listo".into());
            };
            let body = r.viewport.body;
            let (x, y) = (body.x + body.w / 2, body.y + body.h / 2);
            m.preview_pointer_down(x, y, false);
            m.preview_pointer_drag(x - 48, y - 36);
            if let Some(v) = m.preview.bitmap_viewport_mut() {
                v.drag = None;
            }
        }
        "video-assert-transform" | "video-assert-transform-reset" => {
            let View::Video(r) = m.preview.view() else {
                return Err("Vídeo no listo".into());
            };
            let expected = if action.ends_with("-reset") {
                (1., 0., 0.)
            } else {
                (2., 48., 36.)
            };
            let actual = (r.viewport.zoom.max(1.), r.viewport.pan_x, r.viewport.pan_y);
            if actual != expected {
                return Err(format!("Encuadre inesperado: {actual:?} != {expected:?}"));
            }
            eprintln!(
                "FILES_VIDEO_TRANSFORM_OK zoom={} pan={},{}",
                actual.0, actual.1, actual.2
            );
        }
        "video-mid" => m.preview.seek_video_to(2.),
        "video-play" | "video-pause" => m.action(Action::PreviewVideoToggle),
        "video-assert-playing" => {
            let View::Video(r) = m.preview.view() else {
                return Err("Vídeo no listo".into());
            };
            if !m.preview.video_playing() || r.sequence.is_none_or(|n| n < 4) {
                return Err("No se recibieron frames de reproducción".into());
            }
            eprintln!(
                "FILES_VIDEO_PLAYING_OK sequence={:?} position={}",
                r.sequence, r.frame.position
            );
        }
        "video-assert-paused" => {
            if m.preview.video_playing() || !m.preview.is_drained() {
                return Err("Reproducción no pausada/drenada".into());
            }
            eprintln!("FILES_VIDEO_PAUSED_DRAINED");
        }
        "close-video-playing" => {
            if !m.preview.video_playing() {
                return Err("No se estaba reproduciendo".into());
            }
            m.preview.close();
            m.closing = true;
        }
        "video-forward" => m.action(Action::PreviewVideoSeek(5.0)),
        "video-end" => m.preview.seek_video_to(f64::MAX),
        "video-back" => m.action(Action::PreviewVideoSeek(-5.0)),
        "video-quarter" => {
            let View::Video(r) = m.preview.view() else {
                return Err("Vídeo no listo".into());
            };
            let timeline = r.timeline;
            m.preview_pointer_down(
                timeline.x + timeline.w / 4,
                timeline.y + timeline.h / 2,
                false,
            );
        }
        "video-rapid" => {
            m.preview.seek_video_to(1.0);
            m.preview.seek_video_to(6.0);
        }
        "video-bad" => open(m, "bad.mkv")?,
        "video-symlink" => open(m, "symlink.mkv")?,
        "video-assert-red" | "video-assert-blue" => {
            let View::Video(r) = m.preview.view() else {
                return Err("Vídeo no listo".into());
            };
            let red = action == "video-assert-red";
            let count = r
                .frame
                .bitmap
                .rgb
                .as_chunks::<3>()
                .0
                .iter()
                .filter(|p| {
                    if red {
                        p[0] > 180 && p[2] < 70
                    } else {
                        p[2] > 180 && p[0] < 70
                    }
                })
                .count();
            if count * 10 < r.frame.bitmap.rgb.len() / 3 * 9 {
                return Err("Fotograma no corresponde al seek esperado".into());
            }
            eprintln!(
                "FILES_VIDEO_FRAME_OK red={} position={:.6} bytes={}",
                red,
                r.frame.position,
                r.frame.bitmap.rgb.len()
            );
        }
        "close-video-pending" => {
            m.preview.seek_video_by(0.5);
            m.preview.pump();
            m.preview.close();
            m.closing = true;
        }
        "html" => open(m, "layout.html")?,
        "html-source-a" | "html-source-b" => {
            let View::Html(r) = m.preview.view() else {
                return Err("HTML fuente no listo".into());
            };
            let expected = if action == "html-source-a" {
                "FD_ORIGINAL_A"
            } else {
                "PATH_REPLACEMENT_B"
            };
            if !r.rendered.document.text().contains(expected) {
                return Err("Texto de otro original HTML".into());
            }
            eprintln!(
                "FILES_HTML_SOURCE_OK text={} revision={:?}",
                expected, r.source
            );
        }
        "html-source-empty" => {
            if !matches!(m.preview.view(), View::Loading | View::Error(_)) {
                return Err("HTML obsoleto visible".into());
            }
            eprintln!("FILES_HTML_SOURCE_EMPTY_OK");
        }
        "html-source" => open(m, "source.html")?,
        "html-symlink" => open(m, "symlink.html")?,
        "html-blocked" => open(m, "blocked.html")?,
        "html-assert-blocked" => {
            let View::Html(r) = m.preview.view() else {
                return Err("HTML no está listo".into());
            };
            if r.rendered.stats.blocked_css < 1
                || r.rendered.stats.blocked_images < 2
                || r.rendered
                    .document
                    .text()
                    .contains("SCRIPT_EXECUTED_SENTINEL")
            {
                return Err("Política de recursos HTML falló".into());
            }
            eprintln!(
                "FILES_HTML_BLOCKED_OK css={} images={} script_visible=false",
                r.rendered.stats.blocked_css, r.rendered.stats.blocked_images
            );
        }
        "html-assert-end" => {
            let View::Html(r) = m.preview.view_mut() else {
                return Err("HTML no está listo".into());
            };
            let offset = r
                .rendered
                .document
                .text()
                .find("FINAL_HTML_VISIBLE")
                .ok_or("Falta final del documento HTML")?;
            let (_, y) = r.rendered.point_for_offset(&mut FltkPainter, offset);
            let body = m.text_viewport.body;
            if y < body.y || y >= body.y + body.h {
                return Err("Final HTML fuera del viewport tras scroll".into());
            }
            eprintln!("FILES_HTML_FINAL_VISIBLE top={}", r.rendered.top());
        }
        "html-long" => open(m, "long.html")?,
        "html-large" => open(m, "large.html")?,
        "html-scroll" => {
            let View::Html(r) = m.preview.view_mut() else {
                return Err("HTML no está listo".into());
            };
            r.rendered.scroll(i32::MAX);
        }
        "html-select" => {
            let View::Html(r) = m.preview.view_mut() else {
                return Err("HTML no está listo".into());
            };
            let wanted = "HTML/CSS real: título";
            let start = r
                .rendered
                .document
                .text()
                .find(wanted)
                .ok_or("Falta texto HTML visible")?;
            let a = r.rendered.point_for_offset(&mut FltkPainter, start);
            let b = r
                .rendered
                .point_for_offset(&mut FltkPainter, start + wanted.len());
            m.preview_pointer_down(a.0, a.1, false);
            m.preview_pointer_drag(b.0, b.1);
            m.text_viewport.dragging = false;
            if m.preview
                .document()
                .ok_or("HTML desapareció")?
                .selected_text()
                != wanted
            {
                return Err("Selección HTML incorrecta".into());
            }
            eprintln!("FILES_HTML_SELECTION_OK utf8=true clipboard=not-validated");
        }
        "html-rapid" => {
            open(m, "layout.html")?;
            m.preview.pump();
            open(m, "long.html")?;
            open(m, "layout.html")?;
        }
        "close-html-pending" => {
            open(m, "long.html")?;
            m.preview.pump();
            m.closing = true;
        }
        "markdown" => open(m, "overview.md")?,
        "markdown-embedded" => open(m, "embedded.md")?,
        "markdown-contextual" => open(m, "contextual-links.md")?,
        "markdown-context-guide" => follow_named(m, "Guía en subcarpeta")?,
        "markdown-context-detail" => follow_named(m, "Abrir detalle anidado")?,
        "markdown-context-back" => follow_named(m, "Volver a la guía")?,
        "markdown-context-root" => follow_named(m, "Volver al documento inicial")?,
        "markdown-context-reject" => {
            let before = m.preview.path().map(std::path::Path::to_path_buf);
            let revision = m.preview.revision();
            let text = m.preview.document().ok_or("Documento ausente")?.text();
            let label = if text.contains("Intento de salir") {
                "Intento de salir"
            } else if text.contains("Intentar salir") {
                "Intentar salir"
            } else {
                "Escape que debe rechazarse"
            };
            follow_named(m, label)?;
            if m.preview.revision() != revision
                || m.preview.path() != before.as_deref()
                || !m.preview.is_drained()
            {
                return Err("Escape alteró preview o lanzó recurso".into());
            }
            if !matches!(m.preview.view(),View::Markdown(r) if r.notice.as_ref().is_some_and(|s|s.contains("bloqueado")))
            {
                return Err("Escape sin diagnóstico visible".into());
            }
            eprintln!("FILES_MARKDOWN_CONTEXT_REJECTED revision={revision} helper_spawn=false");
        }
        "markdown-source-proof" => {
            let View::Markdown(ready) = m.preview.view() else {
                return Err("Documento aún no publicado".into());
            };
            let proof = ready.source.ok_or("Firma del documento ausente")?;
            let values=ready.resources.iter().map(|(path,proof)|serde_json::json!({"relative_raw":path.as_os_str().as_encoded_bytes(),"proof":proof})).collect::<Vec<_>>();
            eprintln!(
                "FILES_DOCUMENT_PROOF {}",
                serde_json::json!({"revision":m.preview.revision(),"document":proof,"resources":values,"published_after_watch_probe":true})
            );
        }
        "markdown-image-assert" => {
            let View::Markdown(ready) = m.preview.view() else {
                return Err("Markdown no está listo".into());
            };
            if ready.rendered.loaded_images < 1
                || ready.rendered.image_bytes == 0
                || ready.rendered.image_bytes > 4 * 1024 * 1024
            {
                return Err("RGB ausente o fuera de presupuesto".into());
            }
            eprintln!(
                "FILES_MARKDOWN_IMAGES_OK loaded={} retained_bytes={} buffer_bytes={}",
                ready.rendered.loaded_images,
                ready.rendered.image_bytes,
                ready.rendered.image_buffer_capacity()
            );
        }
        "markdown-large" => open(m, "large.md")?,
        "markdown-too-many" => open(m, "too-many.md")?,
        "markdown-symlink" => {
            m.preview
                .follow_link(crate::preview_link::Target::Relative {
                    path: "linked.md".into(),
                    fragment: None,
                });
        }
        "markdown-assert-selection" => {
            let selected = m
                .preview
                .document()
                .ok_or("Documento ausente")?
                .selected_text();
            if selected != "café — pingüino — 漢字 — 👩‍💻 — á" {
                return Err("Resize perdió selección lógica".into());
            }
            eprintln!("FILES_MARKDOWN_SELECTION_AFTER_RESIZE_OK");
        }
        "markdown-select" => {
            let View::Markdown(ready) = m.preview.view_mut() else {
                return Err("Markdown no está listo".into());
            };
            let needle = "café — pingüino — 漢字 — 👩‍💻 — á";
            let start = ready
                .rendered
                .document
                .text()
                .find(needle)
                .ok_or("No se encontró selección Unicode")?;
            let stop = start + needle.len();
            let a = ready.rendered.point_for_offset(&mut FltkPainter, start);
            let b = ready.rendered.point_for_offset(&mut FltkPainter, stop);
            m.preview_pointer_down(a.0, a.1, false);
            // End just beyond the zero-advance combining mark; scalar hit tests
            // cannot distinguish its coincident boundary from the base letter.
            m.preview_pointer_drag(b.0 + 1, b.1);
            if let View::Markdown(ready) = m.preview.view_mut() {
                ready.rendered.viewport.dragging = false;
            }
            let selected = m
                .preview
                .document()
                .ok_or("Documento desapareció")?
                .selected_text();
            if selected != needle {
                return Err(format!("Selección visible incorrecta: {selected:?}"));
            }
            eprintln!(
                "FILES_MARKDOWN_SELECTION bytes={} utf8=true combining=true zwj_preserved=true clipboard=not-validated",
                selected.len()
            );
        }
        "markdown-scroll" => {
            let View::Markdown(ready) = m.preview.view_mut() else {
                return Err("Markdown no está listo".into());
            };
            ready.rendered.scroll(8);
        }
        "markdown-bottom" => {
            let View::Markdown(ready) = m.preview.view_mut() else {
                return Err("Markdown no está listo".into());
            };
            ready.rendered.scroll(i32::MAX);
        }
        "markdown-top" => {
            m.preview.follow_link(crate::preview_link::Target::Fragment(
                "proyecto-aurora".into(),
            ));
        }
        "markdown-guide" | "markdown-return" => {
            let View::Markdown(ready) = m.preview.view_mut() else {
                return Err("Markdown no está listo".into());
            };
            let needle = if action == "markdown-guide" {
                "la guía local"
            } else {
                "de regreso"
            };
            let offset = ready
                .rendered
                .document
                .text()
                .find(needle)
                .ok_or("Enlace fixture no encontrado")?;
            ready.rendered.document.move_to(offset, false);
            ready.rendered.reveal();
            let point = ready
                .rendered
                .point_for_offset(&mut FltkPainter, offset + 1);
            if !m.follow_preview_link(point.0, point.1) {
                return Err("Hit del enlace local falló".into());
            }
        }
        "markdown-rapid" => {
            open(m, "overview.md")?;
            m.preview.pump();
            open(m, "guide.md")?;
            open(m, "overview.md")?;
        }
        "close-markdown-pending" => {
            open(m, "overview.md")?;
            m.preview.pump();
            m.closing = true;
        }
        "png" => open(m, "alpha.png")?,
        "png12" => open(m, "rows12.png")?,
        "png24" => open(m, "rows24.png")?,
        "png24-oriented" => open(m, "rows24-oriented.png")?,
        "png-adam7" => open(m, "adam7.png")?,
        "png-source-assert" => {
            let View::Image(ready) = m.preview.view() else {
                return Err("PNG por filas no está listo".into());
            };
            let b = &ready.bitmap;
            if b.source.is_none()
                || b.format != "png"
                || u64::from(b.source_width) * u64::from(b.source_height) <= 4 * 1024 * 1024
            {
                return Err("PNG sin firma o dimensiones de fuente grande".into());
            }
            let mut colors = std::collections::HashSet::new();
            for rgb in b.rgb.as_chunks::<3>().0 {
                colors.insert(*rgb);
                if colors.len() == 4 {
                    break;
                }
            }
            if colors.len() < 4 {
                return Err("PNG patrón no conserva color/alfa".into());
            }
            eprintln!(
                "FILES_PNG_ROWS_PIXELS width={} height={} colors_at_least=4 actual_source_fd=true",
                b.width, b.height
            );
        }
        "jpeg" => open(m, "oriented.jpg")?,
        "jpeg12" => open(m, "baseline12.jpg")?,
        "jpeg24" => open(m, "baseline24.jpg")?,
        "jpeg24-oriented" => open(m, "oriented24.jpg")?,
        "progressive12" => open(m, "progressive12.jpg")?,
        "progressive24" => open(m, "progressive24.jpg")?,
        "progressive24-oriented" => open(m, "progressive-oriented24.jpg")?,
        "jpeg-progressive-assert" => {
            let View::Image(ready) = m.preview.view() else {
                return Err("JPEG progresivo no está listo".into());
            };
            let bitmap = &ready.bitmap;
            if bitmap.source.is_none() || bitmap.format != "jpeg" || bitmap.source_width < 4000 {
                return Err("JPEG sin firma/dimensiones de origen esperadas".into());
            }
            let mut colors = std::collections::HashSet::new();
            for rgb in bitmap.rgb.as_chunks::<3>().0 {
                colors.insert([rgb[0], rgb[1], rgb[2]]);
                if colors.len() == 33 {
                    break;
                }
            }
            if colors.len() < 33 {
                return Err("JPEG patrón no conserva variación de color".into());
            }
            eprintln!(
                "FILES_JPEG_PROGRESSIVE_PIXELS width={} height={} colors_at_least=33 actual_source_fd=true",
                bitmap.width, bitmap.height
            );
        }
        "close-jpeg-pending" => {
            open(m, "baseline24.jpg")?;
            m.preview.pump();
            m.closing = true;
        }

        "media-red" | "media-blue" | "media-green" => {
            let View::Image(ready) = m.preview.view() else {
                return Err("Imagen no lista".into());
            };
            let expected = match action {
                "media-red" => [220, 30, 40],
                "media-blue" => [20, 50, 230],
                _ => [30, 200, 70],
            };
            if ready.bitmap.source.is_none()
                || ready
                    .bitmap
                    .rgb
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .any(|p| *p != expected)
            {
                return Err("Píxeles o revisión de origen incorrectos".into());
            }
            eprintln!(
                "FILES_MEDIA_PIXELS_OK action={action} generation={} stamp={:?}",
                m.preview.revision(),
                ready.bitmap.source
            );
        }
        "media-trash" => {
            m.action(Action::Trash);
            if m.preview.visible() {
                return Err("Papelera no ocultó preview".into());
            }
            eprintln!("FILES_MEDIA_OWN_DELETE_HIDDEN");
        }
        "media-hide" => {
            m.action(Action::ClosePreview);
            eprintln!("FILES_MEDIA_HIDDEN");
        }
        "webp" => open(m, "tiny.webp")?,
        "image-corrupt" => open(m, "broken.png")?,
        "image-large" => open(m, "large.png")?,
        "image-zoom" => {
            m.action(Action::PreviewImageZoom(2.0));
            let View::Image(ready) = m.preview.view_mut() else {
                return Err("image not ready".into());
            };
            ready.viewport.pan_x = 120.0;
            ready.viewport.pan_y = 80.0;
        }
        "image-rapid" => {
            open(m, "alpha.png")?;
            m.preview.pump();
            open(m, "tiny.webp")?;
            open(m, "oriented.jpg")?;
        }
        "close-image-pending" => {
            open(m, "alpha.png")?;
            m.preview.pump();
            m.closing = true;
        }

        "rapid" => {
            open(m, "A.txt")?;
            m.preview.pump();
            open(m, "B.txt")?;
            open(m, "C.txt")?;
        }
        "binary" => open(m, "binary.bin")?,
        "truncated" => open(m, "long.txt")?,
        "select" => {
            let View::Ready(ready) = m.preview.view() else {
                return Err("text not ready".into());
            };
            if ready.document.line(0) != Some("área\t漢字") {
                return Err("unexpected fixture content".into());
            }
            let start = ready.document.offset(0, 1);
            let stop = ready.document.offset(1, 2);
            let a = m
                .text_viewport
                .point_for_offset(&mut FltkPainter, &ready.document, start);
            let b = m
                .text_viewport
                .point_for_offset(&mut FltkPainter, &ready.document, stop);
            m.preview_pointer_down(a.0, a.1, false);
            m.preview_pointer_drag(b.0, b.1);
            m.text_viewport.dragging = false;
            m.action(Action::CopyPreview);
            let View::Ready(ready) = m.preview.view() else {
                unreachable!()
            };
            eprintln!(
                "FILES_PREVIEW_SELECTION={}",
                serde_json::to_string(ready.document.selected_text()).map_err(|e| e.to_string())?
            );
        }
        "close-preview" => m.action(Action::ClosePreview),
        "close" => {
            m.closing = true;
        }
        "close-pending" => {
            open(m, "A.txt")?;
            m.preview.pump();
            m.closing = true;
        }
        _ => return Err("unknown preview fixture action".into()),
    }
    Ok(())
}
