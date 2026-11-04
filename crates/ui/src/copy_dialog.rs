//! Destination entry for real local copy; selection stays lossless and bounded.
use crate::{
    model::{Action, Hit, Model},
    painter::{Painter, Rect},
};
use std::{ffi::OsString, path::PathBuf, sync::Arc};
pub struct Editor {
    initialized: bool,
    pub cursor: crate::line_edit::Cursor,
    pub geometry: crate::line_field::Geometry,
    pub dragging: bool,
    pub epoch: u64,
}
impl Editor {
    pub fn touch(&mut self) {
        self.epoch = epoch();
    }
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            initialized: false,
            cursor: Default::default(),
            geometry: Default::default(),
            dragging: false,
            epoch: epoch(),
        }
    }
}
fn epoch() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut n = NEXT.load(Ordering::Relaxed);
    loop {
        let next = n.checked_add(1).expect("destination epochs exhausted");
        match NEXT.compare_exchange_weak(n, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(v) => return v,
            Err(v) => n = v,
        }
    }
}
pub struct Dialog {
    pub editor: Editor,
    pub name_field: crate::copy_name_field::Field,
    pub name_focus: bool,
    pub clipboard: Option<Arc<crate::file_clipboard::Lease>>,
    pub multi: Option<Arc<files_core::multi_origin::Captured>>,
    pub moving: bool,
    pub extracting: bool,
    pub creating: bool,
    pub archive: Option<crate::archive_operation::Prepared>,
    pub preparing: bool,
    pub prepare_id: Option<u64>,
    pub prepared: Option<crate::move_operation::Prepared>,
    pub source: PathBuf,
    pub names: Arc<[OsString]>,
    pub destination: String,
    pub captured_destination: Option<PathBuf>,
    pub replace: bool,
    pub start_paused: bool,
    pub options_open: bool,
    pub publication: files_core::copy_engine::Publication,
    pub preserve_symlink_times: bool,
    pub existing_destination: files_core::copy_engine::ExistingDestination,
    pub error: String,
}
impl Dialog {
    pub fn policy_summary(&self) -> &'static str {
        use files_core::copy_engine::Publication as P;
        match (self.publication, self.preserve_symlink_times) {
            (P::StagedRename, true) => {
                "Destino oculto hasta completar cada raíz.\nFechas de enlaces: conservar."
            }
            (P::StagedRename, false) => {
                "Destino oculto hasta completar cada raíz.\nFechas de enlaces: omitir explícitamente."
            }
            (P::VisibleExclusive, true) => {
                "Destino visible: conserva parciales.\nEnlaces: omite fechas o la copia se detendrá."
            }
            (P::VisibleExclusive, false) => {
                "Destino visible: conserva parciales.\nFechas de enlaces: omitir explícitamente."
            }
        }
    }
    pub fn capture_destination(&mut self, path: PathBuf) {
        self.destination = if path
            .to_str()
            .is_some_and(|s| !s.chars().any(char::is_control))
        {
            path.to_str().unwrap().to_owned()
        } else {
            format!("{path:?}")
        };
        if self.destination.len() > 4096 {
            let mut n = 4093;
            while !self.destination.is_char_boundary(n) {
                n -= 1;
            }
            self.destination.truncate(n);
            self.destination.push('…');
        }
        self.captured_destination = Some(path);
        self.replace = true;
        self.editor = Editor::default();
        self.select_all();
        self.prepared = None;
        self.archive = None;
    }
    pub fn destination_path(&self) -> PathBuf {
        self.captured_destination
            .clone()
            .unwrap_or_else(|| PathBuf::from(&self.destination))
    }

    pub fn ensure_editor(&mut self) {
        if !self.editor.initialized {
            self.editor.initialized = true;
            if self.replace {
                self.editor.cursor.all(&self.destination);
            }
        }
    }
    pub fn select_all(&mut self) {
        if self.preparing || self.options_open {
            return;
        }
        self.ensure_editor();
        self.editor.cursor.all(&self.destination);
        self.replace = true;
        self.touch();
    }
    pub fn touch(&mut self) {
        self.editor.epoch = epoch();
    }
    pub fn motion(&mut self, motion: crate::line_edit::Motion, shift: bool) {
        if self.preparing || self.options_open {
            return;
        }
        self.ensure_editor();
        self.editor.cursor.motion(&self.destination, motion, shift);
        self.replace = false;
        self.touch();
    }
    pub fn selected_text(&self) -> &str {
        &self.destination[self.editor.cursor.range(&self.destination)]
    }
    fn opaque(&self) -> bool {
        self.captured_destination
            .as_ref()
            .is_some_and(|p| p.to_str().is_none_or(|s| s.chars().any(char::is_control)))
    }
    fn full_edit(&self) -> bool {
        self.replace || self.editor.cursor.range(&self.destination) == (0..self.destination.len())
    }
    fn changed(&mut self) {
        self.captured_destination = None;
        self.prepared = None;
        self.archive = None;
        self.replace = false;
        self.error.clear();
        self.editor.dragging = false;
        self.touch();
    }
    pub fn input(&mut self, text: &str) {
        if self.preparing || self.options_open {
            self.error = "Espera revisión o cancela el diálogo".into();
            return;
        }
        self.ensure_editor();
        if text.chars().any(char::is_control) {
            self.error = "La ruta debe ser una sola línea sin controles".into();
            return;
        }
        if self.opaque() && !self.full_edit() {
            self.error =
                "Ruta opaca: Ctrl+A para sustituirla completa; no se editan los bytes escapados"
                    .into();
            return;
        }
        let range = if self.replace {
            0..self.destination.len()
        } else {
            self.editor.cursor.range(&self.destination)
        };
        if self
            .destination
            .len()
            .saturating_sub(range.len())
            .checked_add(text.len())
            .is_none_or(|n| n > 4096)
        {
            self.error = "Ruta demasiado larga (máximo4096bytes)".into();
            return;
        }
        if self.opaque() {
            // Escaped presentation is not a recoverable filesystem pathname.
            // Explicit replacement starts a new text undo boundary.
            self.destination.clear();
            self.destination.push_str(text);
            self.editor.cursor = Default::default();
            self.editor
                .cursor
                .motion(&self.destination, crate::line_edit::Motion::End, false);
            self.changed();
        } else {
            let old = self.destination.clone();
            if let Err(e) =
                self.editor
                    .cursor
                    .insert_bounded(&mut self.destination, text, self.replace, 4096)
            {
                self.error = e.into();
                return;
            }
            if old != self.destination {
                self.changed();
            } else {
                self.replace = false;
                self.touch();
            }
        }
    }
    pub fn delete(&mut self, back: bool, word: bool) {
        if self.preparing || self.options_open {
            return;
        }
        self.ensure_editor();
        if self.opaque() {
            self.input("");
            return;
        }
        let old = self.destination.clone();
        self.editor
            .cursor
            .delete(&mut self.destination, back, word, self.replace);
        if old != self.destination {
            self.changed();
        } else {
            self.replace = false;
            self.touch();
        }
    }
    #[cfg(test)]
    pub fn backspace(&mut self) {
        self.delete(true, false);
    }
    pub fn undo(&mut self, redo: bool) {
        if self.preparing || self.opaque() {
            return;
        }
        self.ensure_editor();
        let old = self.destination.clone();
        self.editor.cursor.undo(&mut self.destination, redo);
        if old != self.destination {
            self.changed();
        } else {
            self.touch();
        }
    }
}
fn button(p: &mut dyn Painter, m: &mut Model, r: Rect, label: &str, action: Action) {
    p.rounded_rect(r, 6 * m.radius, m.palette.selection);
    p.text(
        Rect::new(r.x + 10, r.y, r.w - 20, r.h),
        label,
        12,
        m.palette.accent,
        true,
        false,
    );
    m.hits.push(Hit {
        generation: m.generation,
        rect: r,
        action,
    });
}
fn paint_options(p: &mut dyn Painter, m: &mut Model, d: &Dialog, r: Rect) {
    let c = m.palette.clone();
    p.literal_text(
        Rect::new(r.x + 20, r.y + 12, r.w - 40, 32),
        " Opciones de copia",
        16,
        c.bright_foreground,
    );
    p.literal_text(
        Rect::new(r.x + 20, r.y + 52, r.w - 40, 24),
        &if d.extracting {
            format!(
                "ZIP: {} · {}",
                files_core::name_label::label(&d.names[0]),
                files_core::name_label::label(d.source.as_os_str())
            )
        } else if let Some(proof) = &d.multi {
            format!(
                "{} orígenes · {} raíces bajo {}",
                proof.groups().len(),
                d.names.len(),
                files_core::name_label::label(d.source.as_os_str())
            )
        } else {
            format!(
                "Desde: {}",
                files_core::name_label::label(d.source.as_os_str())
            )
        },
        11,
        c.dark_foreground,
    );
    p.literal_text(
        Rect::new(r.x + 20, r.y + 78, r.w - 40, 24),
        &format!(
            "Destino: {}",
            files_core::name_label::label(d.destination_path().as_os_str())
        ),
        11,
        c.dark_foreground,
    );
    let visible = d.publication == files_core::copy_engine::Publication::VisibleExclusive;
    button(
        p,
        m,
        Rect::new(r.x + 20, r.y + 116, r.w - 40, 36),
        if visible {
            " Destino visible durante la copia · 1"
        } else {
            " Destino visible durante la copia · 1"
        },
        Action::CopyVisiblePublication,
    );
    button(
        p,
        m,
        Rect::new(r.x + 20, r.y + 164, r.w - 40, 36),
        if d.preserve_symlink_times {
            " Conservar fechas de enlaces · 2"
        } else {
            " Conservar fechas de enlaces · 2"
        },
        Action::CopySymlinkTimes,
    );
    let explanation = if visible {
        "El destino aparece desde el inicio.\nSi cancelas o falla, conserva los parciales.\nNo se borran automáticamente.\nEl origen se conserva."
    } else {
        "Cada raíz aparece al completar su copia.\nSi el montaje no admite esta publicación,\nla copia se detiene. Nunca cambia de\nmodalidad automáticamente."
    };
    p.text(
        Rect::new(r.x + 20, r.y + 212, r.w - 40, 62),
        explanation,
        11,
        c.foreground,
        true,
        false,
    );
    p.text(Rect::new(r.x+20,r.y+278,r.w-40,58),"Copia visible: desactiva fechas de enlaces.\nArchivos y carpetas conservan sus fechas.\nNo sigue enlaces ni sobrescribe ocupados.",11,c.dark_foreground,true,false);
    button(
        p,
        m,
        Rect::new(r.x + 20, r.y + r.h - 50, r.w - 40, 34),
        " Volver a revisión · F3 / Enter / Esc",
        Action::CopyOptions,
    );
}
fn archive_names(names: &[std::ffi::OsString]) -> String {
    let mut out = String::new();
    for name in names.iter().take(2) {
        if !out.is_empty() {
            out.push_str(" · ");
        }
        let debug = files_core::name_label::label(name);
        out.extend(debug.chars().take(16));
        if debug.chars().count() > 16 {
            out.push('…');
        }
    }
    if names.len() > 2 {
        out.push_str(&format!(" · +{}", names.len() - 2));
    }
    out
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, d: &mut Dialog, w: i32, h: i32) {
    m.hits.clear();
    let c = m.palette.clone();
    let width = (w - 40).min(860);
    let height = (if d.moving || d.extracting { 340 } else { 400 }).min(h - 40);
    let x = (w - width) / 2;
    let y = (h - height) / 2;
    p.rounded_rect(Rect::new(x, y, width, height), 10 * m.radius, c.muted);
    p.rounded_rect(
        Rect::new(x + 1, y + 1, width - 2, height - 2),
        10 * m.radius,
        c.darker_background,
    );
    if d.options_open && !d.moving && !d.extracting {
        paint_options(p, m, d, Rect::new(x, y, width, height));
        return;
    }
    p.text(
        Rect::new(x + 20, y + 12, width - 40, 32),
        &format!(
            "{}{} {} elemento{}{}",
            if d.clipboard.as_ref().is_some_and(|c| c.system) {
                "Sistema · "
            } else if d.clipboard.is_some() {
                "files · "
            } else {
                ""
            },
            if d.creating {
                "󰆧 Crear ZIP"
            } else if d.extracting {
                "󰇚  Extraer ZIP"
            } else if d.moving {
                "󰁔  Mover"
            } else {
                "󰆏  Copiar"
            },
            d.names.len(),
            if d.names.len() == 1 { "" } else { "s" },
            if !d.moving && !d.extracting && d.name_field.changed_count() > 0 {
                format!(
                    " · {} nombre{}",
                    d.name_field.changed_count(),
                    if d.name_field.changed_count() == 1 {
                        ""
                    } else {
                        "s"
                    }
                )
            } else {
                String::new()
            }
        ),
        16,
        c.bright_foreground,
        true,
        false,
    );
    let caption = if d.creating {
        format!(
            "Origen: {}\n{}",
            files_core::name_label::label(d.source.as_os_str()),
            archive_names(&d.names)
        )
    } else if d.extracting {
        format!(
            "ZIP: {}\nCarpeta: {}",
            files_core::name_label::label(&d.names[0]),
            files_core::name_label::label(d.source.as_os_str())
        )
    } else if let Some(proof) = &d.multi {
        format!(
            "{} orígenes · {} raíces bajo {}",
            proof.groups().len(),
            d.names.len(),
            files_core::name_label::label(d.source.as_os_str())
        )
    } else {
        format!(
            "Desde: {}",
            files_core::name_label::label(d.source.as_os_str())
        )
    };
    if d.creating || d.extracting {
        for (row, line) in caption.lines().take(2).enumerate() {
            p.literal_text(
                Rect::new(x + 20, y + 52 + row as i32 * 14, width - 40, 14),
                line,
                11,
                c.dark_foreground,
            );
        }
    } else {
        p.literal_text(
            Rect::new(x + 20, y + 52, width - 40, 26),
            &caption,
            11,
            c.dark_foreground,
        );
    }
    p.text(
        Rect::new(x + 20, y + 90, width - 40, 24),
        if d.opaque() {
            "Ruta raw · Ctrl+A para sustituir completa"
        } else {
            if d.creating {
                "Nuevo archivo ZIP · no puede existir"
            } else if d.extracting {
                "Nueva carpeta destino · no puede existir"
            } else if d.moving {
                "Destino · Ctrl+A · Ctrl+Z · Ctrl+←/→"
            } else {
                "Carpeta destino · Ctrl+L · F2 nombre"
            }
        },
        11,
        c.foreground,
        true,
        false,
    );
    p.rounded_rect(
        Rect::new(x + 20, y + 120, width - 40, 38),
        6 * m.radius,
        c.selection,
    );
    d.ensure_editor();
    d.editor.geometry.paint(
        p,
        &d.destination,
        &d.editor.cursor,
        Rect::new(x + 20, y + 120, width - 40, 38),
        &c,
        (!d.preparing && !d.name_focus, d.editor.epoch),
    );
    m.hits.push(Hit {
        generation: m.generation,
        rect: Rect::new(x + 20, y + 120, width - 40, 38),
        action: Action::CopyDestination,
    });
    if !d.moving && !d.extracting {
        let e = d.name_field.editor.epoch;
        p.literal_text(
            Rect::new(x + 20, y + 162, width - 164, 20),
            &format!(
                "Destino {}/{} ← {}",
                d.name_field.row + 1,
                d.names.len(),
                d.name_field.original_label()
            ),
            11,
            c.dark_foreground,
        );
        for (dx, label, action) in [
            (128, "", Action::CopyNameStep(e, -1)),
            (92, "", Action::CopyNameStep(e, 1)),
            (56, "", Action::CopyNameReset(e)),
        ] {
            button(
                p,
                m,
                Rect::new(x + width - dx, y + 158, 32, 26),
                label,
                action,
            );
        }
        let area = Rect::new(x + 20, y + 188, width - 40, 32);
        p.rounded_rect(area, 6 * m.radius, c.selection);
        d.name_field.editor.geometry.paint(
            p,
            &d.name_field.draft,
            &d.name_field.editor.cursor,
            area,
            &c,
            (d.name_focus, d.name_field.editor.epoch),
        );
        m.hits.push(Hit {
            generation: m.generation,
            rect: area,
            action: Action::CopyNameFocus(e),
        });
    }
    button(
        p,
        m,
        Rect::new(
            x + 20,
            y + if d.moving || d.extracting { 172 } else { 230 },
            if d.moving || d.extracting {
                230
            } else {
                (width - 48) / 2
            },
            32,
        ),
        if d.start_paused {
            crate::fonts::icon("  Iniciar en pausa", "[x] Iniciar en pausa")
        } else {
            crate::fonts::icon("  Iniciar en pausa", "[ ] Iniciar en pausa")
        },
        Action::CopyStartPaused,
    );
    if !d.moving && !d.extracting {
        let half = (width - 48) / 2;
        button(
            p,
            m,
            Rect::new(x + 28 + half, y + 230, half, 32),
            " Opciones · F3",
            Action::CopyOptions,
        );
        button(
            p,
            m,
            Rect::new(x + 20, y + 266, width - 40, 30),
            match d.existing_destination {
                files_core::copy_engine::ExistingDestination::Stop => " Detener si existe · F4",
                files_core::copy_engine::ExistingDestination::Skip => " Omitir existentes · F4",
                files_core::copy_engine::ExistingDestination::KeepBoth => " Conservar ambos · F4",
            },
            Action::CopySkipExisting,
        );
    }
    let archive_summary = d.archive.as_ref().map(|a| a.summary());
    let message = if !d.error.is_empty() {
        &d.error
    } else if d.preparing {
        "Revisando origen y destino…"
    } else if d.creating {
        archive_summary
            .as_deref()
            .unwrap_or("ZIP32 DEFLATE. Revisa antes de crear; nunca sobrescribe. Sin ACL/xattrs.")
    } else if d.extracting {
        archive_summary.as_deref().unwrap_or(
            "Sólo ZIP32. Revisar no crea salida; Aplicar extrae en carpeta nueva sin sobrescribir.",
        )
    } else if !d.moving {
        d.policy_summary()
    } else if let Some(prepared) = &d.prepared {
        if prepared.context().is_some() {
            "EXDEV: un archivo, copia verificada y journal. Confirma para encolar; no sobrescribe."
        } else {
            "Revisión lista. Confirma para encolar; rename nativo sin sobrescritura."
        }
    } else {
        "Revisa el destino. Rename nativo o EXDEV de un archivo regular sin hardlinks; no sobrescribe."
    };
    p.text(
        Rect::new(
            x + 20,
            y + if d.moving || d.extracting { 214 } else { 302 },
            width - 40,
            if d.moving || d.extracting { 48 } else { 34 },
        ),
        message,
        11,
        if d.error.is_empty() {
            c.dark_foreground
        } else {
            c.red
        },
        true,
        false,
    );
    button(
        p,
        m,
        Rect::new(x + width - 368, y + height - 50, 126, 34),
        " Cancelar",
        Action::CopyDialogCancel,
    );
    button(
        p,
        m,
        Rect::new(x + width - 226, y + height - 50, 206, 34),
        if d.creating {
            if d.preparing {
                "Revisando…"
            } else if d.archive.is_some() {
                "󰄬 Crear ZIP"
            } else {
                "󰆧 Revisar creación"
            }
        } else if d.extracting {
            if d.preparing {
                "Revisando…"
            } else if d.archive.is_some() {
                "󰄬 Aplicar extracción"
            } else {
                "󰇚 Revisar ZIP"
            }
        } else if d.moving {
            if d.preparing {
                "Revisando…"
            } else if d.prepared.is_some() {
                "Confirmar y encolar"
            } else {
                "Revisar movimiento"
            }
        } else {
            if d.publication == files_core::copy_engine::Publication::VisibleExclusive {
                "󰆏 Encolar copia visible"
            } else {
                "󰆏 Encolar copia"
            }
        },
        if d.extracting {
            Action::ApplyArchive(d.editor.epoch, d.prepare_id)
        } else {
            Action::CopyStart
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dialog() -> Dialog {
        Dialog {
            editor: Default::default(),
            name_field: crate::copy_name_field::Field::new(Arc::from([])),
            name_focus: false,
            clipboard: None,
            multi: None,
            moving: false,
            extracting: false,
            creating: false,
            archive: None,
            preparing: false,
            prepare_id: None,
            prepared: None,
            source: "/source".into(),
            names: Arc::from([]),
            destination: "/keep".into(),
            captured_destination: None,
            replace: true,
            start_paused: false,
            options_open: false,
            publication: files_core::copy_engine::Publication::StagedRename,
            preserve_symlink_times: true,
            existing_destination: files_core::copy_engine::ExistingDestination::Stop,
            error: String::new(),
        }
    }
    #[cfg(unix)]
    #[test]
    fn captured_raw_destination_survives_rejected_edits_until_valid_explicit_edit() {
        use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
        let mut d = dialog();
        let raw = PathBuf::from(OsStr::from_bytes(b"/destination-\xfe"));
        d.capture_destination(raw.clone());
        d.input(&"x".repeat(4097));
        assert_eq!(d.destination_path(), raw);
        assert!(d.replace);
        d.input("bad\ncontrol");
        assert_eq!(d.destination_path(), raw);
        d.input("/explicit");
        assert!(d.captured_destination.is_none());
        assert_eq!(d.destination_path(), PathBuf::from("/explicit"));
        d.capture_destination(raw);
        d.backspace();
        assert_eq!(d.destination_path(), PathBuf::new());
    }
    #[test]
    fn paste_preflight_rejects_before_mutating_or_allocating_filtered_copy() {
        let mut d = dialog();
        d.input(&"é".repeat(100_000));
        assert_eq!(d.destination, "/keep");
        assert!(d.replace);
        assert!(!d.error.is_empty());
        d.input("/a\nb");
        assert_eq!(d.destination, "/keep");
        assert!(d.replace);
        d.input(&"é".repeat(2048));
        assert_eq!(d.destination.len(), 4096);
        assert!(!d.replace);
        let before = d.destination.clone();
        d.input("a");
        assert_eq!(d.destination, before);
        assert!(!d.replace);
    }
    #[test]
    fn destination_midline_unicode_selection_undo_and_rejected_input() {
        use crate::line_edit::Motion;
        let mut d = dialog();
        d.input("/a/ñ/file");
        d.motion(Motion::Start, false);
        for _ in 0..3 {
            d.motion(Motion::Right, false);
        }
        d.motion(Motion::Right, true);
        assert_eq!(d.selected_text(), "ñ");
        let selected = d.editor.cursor.range(&d.destination);
        let epoch = d.editor.epoch;
        d.input(&"x".repeat(4097));
        assert_eq!(d.destination, "/a/ñ/file");
        assert_eq!(d.editor.cursor.range(&d.destination), selected);
        assert_eq!(d.editor.epoch, epoch);
        d.input("nuevo");
        assert_eq!(d.destination, "/a/nuevo/file");
        d.undo(false);
        assert_eq!(d.destination, "/a/ñ/file");
        d.undo(true);
        assert_eq!(d.destination, "/a/nuevo/file");
        d.motion(Motion::End, false);
        d.delete(true, true);
        assert_eq!(d.destination, "/a/nuevo/");
    }
    #[cfg(unix)]
    #[test]
    fn opaque_destination_caret_is_not_a_lossy_edit_and_replacement_resets_undo() {
        use crate::line_edit::Motion;
        use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
        let mut d = dialog();
        let raw = PathBuf::from(OsStr::from_bytes(b"/raw-\xff"));
        d.capture_destination(raw.clone());
        d.motion(Motion::End, false);
        d.input("suffix");
        assert_eq!(d.destination_path(), raw);
        assert!(d.error.contains("opaca"));
        d.select_all();
        d.input("/explicit");
        d.undo(false);
        assert_eq!(d.destination_path(), PathBuf::from("/explicit"));
        assert!(d.captured_destination.is_none());
    }
    #[test]
    fn destination_editor_nonce_and_preparing_guard() {
        use crate::line_edit::Motion;
        let mut a = dialog();
        let mut b = dialog();
        a.ensure_editor();
        b.ensure_editor();
        assert_ne!(a.editor.epoch, b.editor.epoch);
        let before = a.destination.clone();
        a.preparing = true;
        a.motion(Motion::End, false);
        a.select_all();
        a.input("bad");
        a.backspace();
        a.undo(false);
        assert_eq!(a.destination, before);
    }
    #[cfg(unix)]
    #[test]
    fn captured_control_and_expanded_raw_display_are_bounded_and_never_reparsed() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let mut d = dialog();
        let control = PathBuf::from("/a\nb");
        d.capture_destination(control.clone());
        assert!(!d.destination.contains('\n'));
        d.motion(crate::line_edit::Motion::End, false);
        d.input("x");
        assert_eq!(d.destination_path(), control);
        let raw = PathBuf::from(OsString::from_vec(
            [b"/".as_slice(), &vec![0xff; 4000]].concat(),
        ));
        d.capture_destination(raw.clone());
        assert!(d.destination.len() <= 4096 && d.destination.ends_with('…'));
        assert_eq!(d.destination_path(), raw);
        d.select_all();
        d.input("/valid");
        assert_eq!(d.destination_path(), PathBuf::from("/valid"));
    }
}
