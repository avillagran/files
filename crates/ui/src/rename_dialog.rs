//! Custom-painted rename/create form. A preview never applies itself.
use crate::{
    model::{Action, Hit, Model},
    painter::{Painter, Rect},
};
use files_core::{
    rename_plan::{Case, RenameOptions},
    rename_transaction::PreparedRename,
};
use std::{ffi::OsString, path::PathBuf, sync::Arc, time::Instant};
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Rename,
    File,
    Folder,
}
pub struct Dialog {
    pub id: u64,
    pub directory: PathBuf,
    pub selected: Arc<[OsString]>,
    pub mode: Mode,
    pub fields: [String; 5],
    pub focus: usize,
    pub replace: bool,
    pub case: Case,
    pub regex: bool,
    pub prepared: Option<PreparedRename>,
    pub error: String,
    pub changed: bool,
    pub edited_at: Instant,
    pub scroll: usize,
}
impl Dialog {
    pub fn new(id: u64, directory: PathBuf, selected: Arc<[OsString]>, mode: Mode) -> Self {
        Self {
            id,
            directory,
            selected,
            mode,
            fields: [
                if mode == Mode::Rename {
                    "{name}{dotext}"
                } else {
                    ""
                }
                .into(),
                "1".into(),
                "1".into(),
                String::new(),
                String::new(),
            ],
            focus: 0,
            replace: true,
            case: Case::Preserve,
            regex: false,
            prepared: None,
            error: String::new(),
            changed: mode == Mode::Rename,
            edited_at: Instant::now(),
            scroll: 0,
        }
    }
    pub fn change(&mut self) {
        // Zero invalidates the prior preview immediately. The model allocates a
        // fresh process-global ID when submitting this revision.
        self.id = 0;
        self.prepared = None;
        self.error.clear();
        self.changed = self.mode == Mode::Rename;
        self.edited_at = Instant::now();
        self.scroll = 0;
    }
    pub fn input(&mut self, text: &str) {
        let size = if self.replace {
            0
        } else {
            self.fields[self.focus].len()
        };
        // Valida bytes UTF-8 prestados antes de reservar o modificar el campo.
        // Conserva la limpieza histórica de saltos de línea y NUL.
        let clean = || text.chars().filter(|c| !matches!(c, '\n' | '\r' | '\0'));
        let mut total = size;
        for c in clean() {
            let Some(next) = total.checked_add(c.len_utf8()).filter(|n| *n <= 2048) else {
                self.error = "Campo demasiado largo (máximo 2048 bytes)".into();
                return;
            };
            total = next;
        }
        if self.replace {
            self.fields[self.focus].clear();
            self.replace = false;
        }
        self.fields[self.focus].extend(clean());
        self.change();
    }
    pub fn backspace(&mut self) {
        if self.replace {
            self.fields[self.focus].clear();
            self.replace = false;
        } else {
            self.fields[self.focus].pop();
        }
        self.change();
    }
    pub fn next_field(&mut self, backwards: bool) {
        let n = if self.mode == Mode::Rename { 5 } else { 1 };
        self.focus = (self.focus + if backwards { n - 1 } else { 1 }) % n;
        self.replace = true;
    }
    pub fn options(&self) -> Result<RenameOptions, String> {
        let start = self.fields[1]
            .parse::<u64>()
            .map_err(|_| "Inicio debe ser entero positivo o cero")?;
        let step = self.fields[2]
            .parse::<u64>()
            .map_err(|_| "Paso debe ser entero positivo o cero")?;
        Ok(RenameOptions {
            pattern: self.fields[0].clone(),
            start,
            step,
            replace: (!self.fields[3].is_empty() || self.regex)
                .then(|| (self.fields[3].clone(), self.fields[4].clone())),
            case: self.case,
            regex: self.regex,
            case_insensitive: false,
            portable_names: true,
        })
    }
}
fn r(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect::new(x, y, w, h)
}
fn target(m: &mut Model, rect: Rect, action: Action) {
    m.hits.push(Hit {
        generation: m.generation,
        rect,
        action,
    });
}
fn button(
    p: &mut dyn Painter,
    m: &mut Model,
    rect: Rect,
    text: &str,
    action: Action,
    enabled: bool,
) {
    p.rounded_rect(
        rect,
        6 * m.radius,
        if enabled {
            m.palette.selection
        } else {
            m.palette.lighter_background
        },
    );
    p.text(
        r(rect.x + 10, rect.y, rect.w - 20, rect.h),
        text,
        12,
        if enabled {
            m.palette.accent
        } else {
            m.palette.dark_foreground
        },
        true,
        false,
    );
    target(m, rect, action);
}
fn error_line(error: &str) -> String {
    let mut output = String::new();
    let mut whitespace = false;
    for ch in error.chars() {
        if ch.is_whitespace() || ch.is_control() {
            whitespace = !output.is_empty();
        } else {
            let extra = ch.len_utf8() + usize::from(whitespace);
            if output.len().saturating_add(extra) > 509 {
                output.push('…');
                break;
            }
            if whitespace {
                output.push(' ');
            }
            whitespace = false;
            output.push(ch);
        }
    }
    output
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, d: &Dialog, w: i32, h: i32) {
    m.hits.clear();
    let c = m.palette.clone();
    let width = (w - 36).min(900);
    let height = (h - 50).min(590);
    let x = (w - width) / 2;
    let y = (h - height) / 2;
    p.rounded_rect(r(x, y, width, height), 10 * m.radius, c.muted);
    p.rounded_rect(
        r(x + 1, y + 1, width - 2, height - 2),
        10 * m.radius,
        c.darker_background,
    );
    let title = match d.mode {
        Mode::Rename => format!(
            "{}  Renombrar {} elementos",
            crate::fonts::icon("\u{f044}", ""),
            d.selected.len()
        ),
        Mode::File => "Crear archivo".into(),
        Mode::Folder => "Crear carpeta".into(),
    };
    p.text(
        r(x + 20, y + 10, width - 40, 30),
        &title,
        16,
        c.bright_foreground,
        true,
        false,
    );
    p.literal_text(
        r(x + 20, y + 43, width - 40, 24),
        &files_core::name_label::label(d.directory.as_os_str()),
        11,
        c.dark_foreground,
    );
    let count = if d.mode == Mode::Rename { 5 } else { 1 };
    for i in 0..count {
        let (fx, fy, fw, label) = match i {
            0 => (
                x + 20,
                y + 82,
                width - 40,
                if d.mode == Mode::Rename {
                    "Patrón: {name} {dotext} {ext} {n} {n:03}"
                } else {
                    "Nombre"
                },
            ),
            1 => (x + 20, y + 146, 100, "Inicio"),
            2 => (x + 132, y + 146, 100, "Paso"),
            3 => (
                x + 248,
                y + 146,
                (width - 288) / 2,
                if d.regex {
                    "Expresión regular"
                } else {
                    "Reemplazar literal"
                },
            ),
            _ => (
                x + 258 + (width - 288) / 2,
                y + 146,
                (width - 288) / 2,
                "Por",
            ),
        };
        p.text(
            r(fx, fy - 18, fw, 18),
            label,
            10,
            c.dark_foreground,
            true,
            false,
        );
        let boxr = r(fx, fy, fw, 32);
        p.rounded_rect(
            boxr,
            5 * m.radius,
            if d.focus == i {
                c.selection
            } else {
                c.lighter_background
            },
        );
        let value = if d.focus == i {
            format!("{}│", d.fields[i])
        } else {
            d.fields[i].clone()
        };
        p.text(
            r(fx + 8, fy, fw - 16, 32),
            &value,
            12,
            c.bright_foreground,
            true,
            false,
        );
        target(m, boxr, Action::RenameField(i));
    }
    if d.mode == Mode::Rename {
        let label = match d.case {
            Case::Preserve => "Caso: conservar",
            Case::Lower => "Caso: minúsculas",
            Case::Upper => "Caso: MAYÚSCULAS",
        };
        button(
            p,
            m,
            r(x + 20, y + 190, 212, 30),
            label,
            Action::RenameCase,
            true,
        );
        button(
            p,
            m,
            r(x + 248, y + 190, width - 268, 30),
            if d.regex {
                "Regex · capturas $1 ${nombre} $$"
            } else {
                "Reemplazo literal · activar regex"
            },
            Action::RenameRegex,
            true,
        );
        p.text(
            r(x + 20, y + 230, (width - 50) / 2, 24),
            "ANTES",
            10,
            c.dark_foreground,
            true,
            false,
        );
        p.text(
            r(x + width / 2 + 10, y + 230, width / 2 - 30, 24),
            "DESPUÉS",
            10,
            c.dark_foreground,
            true,
            false,
        );
        let top = y + 258;
        let bottom = y + height - 100;
        p.clip(r(x + 20, top, width - 40, (bottom - top).max(0)));
        if let Some(plan) = &d.prepared {
            if plan.is_empty() {
                p.text(
                    r(x + 20, top, width - 40, 28),
                    "El patrón no cambia ningún nombre.",
                    12,
                    c.dark_foreground,
                    true,
                    false,
                );
            }
            for (row, (from, to)) in plan
                .renames()
                .skip(d.scroll)
                .take(((bottom - top).max(0) / 28) as usize + 1)
                .enumerate()
            {
                let ry = top + row as i32 * 28;
                if row % 2 == 0 {
                    p.rect(r(x + 20, ry, width - 40, 28), c.lighter_background);
                }
                p.literal_text(
                    r(x + 26, ry, (width - 64) / 2, 28),
                    &files_core::name_label::label(from),
                    12,
                    c.foreground,
                );
                p.literal_text(
                    r(x + width / 2 + 10, ry, width / 2 - 36, 28),
                    &files_core::name_label::label(to),
                    12,
                    c.green,
                );
            }
        } else {
            p.text(
                r(x + 20, top, width - 40, 34),
                if m.mutations.busy {
                    "Calculando vista previa fuera de la UI…"
                } else if !d.error.is_empty() {
                    "Sin cambios aplicables. Corrige el error indicado debajo."
                } else {
                    "Edita el patrón para calcular una vista previa."
                },
                12,
                c.dark_foreground,
                true,
                false,
            );
        }
        p.unclip();
    }
    let footer = y + height - 84;
    // FLTK's multiline text alignment can hide the first line in a one-line
    // footer. Keep the original diagnostic; draw a bounded, readable summary.
    let diagnostic = error_line(&d.error);
    let status = if m.mutations.busy {
        "Operación en curso; Cancelar solicita rollback seguro entre pasos."
    } else if !d.error.is_empty() {
        &diagnostic
    } else if d.regex {
        "Regex UTF-8: usa rangos explícitos; clases y casefold Unicode no disponibles."
    } else {
        "La vista previa no modifica archivos. Aplicar vuelve a verificar identidades."
    };
    p.text(
        r(x + 20, footer, width - 40, 28),
        status,
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
        r(x + width - 290, y + height - 46, 120, 32),
        if m.mutations.busy {
            "Cancelar tarea"
        } else {
            "Cancelar"
        },
        Action::RenameCancel,
        true,
    );
    let enabled = !m.mutations.busy
        && (d.mode != Mode::Rename || d.prepared.as_ref().is_some_and(|p| !p.is_empty()));
    button(
        p,
        m,
        r(x + width - 158, y + height - 46, 138, 32),
        if d.mode == Mode::Rename {
            "Aplicar cambios"
        } else {
            "Crear"
        },
        Action::RenameApply,
        enabled,
    );
}

#[cfg(test)]
mod input_tests {
    use super::*;

    fn dialog() -> Dialog {
        Dialog::new(17, PathBuf::from("/fixture"), Arc::from([]), Mode::Rename)
    }

    #[test]
    fn oversized_paste_preserves_field_and_preview_revision() {
        let mut d = dialog();
        d.changed = false;
        let original = d.fields.clone();
        d.input(&"x".repeat(1024 * 1024));
        assert_eq!(d.fields, original);
        assert_eq!(d.id, 17);
        assert!(d.replace);
        assert!(!d.changed);
        assert!(!d.error.is_empty());
    }

    #[test]
    fn unicode_limit_and_control_cleanup_preserve_source_characters() {
        let mut d = dialog();
        let value = "é".repeat(1024);
        d.input(&format!("\r\n{value}\0"));
        assert_eq!(d.fields[0], value);
        assert_eq!(d.fields[0].len(), 2048);
        assert_eq!(d.id, 0);
        assert!(!d.replace);
        assert!(d.changed);
        assert!(d.error.is_empty());
        d.id = 23;
        d.changed = false;
        d.input("é");
        assert_eq!(d.fields[0], value);
        assert_eq!(d.id, 23);
        assert!(!d.changed);
    }

    #[test]
    fn append_counts_existing_bytes_and_replacement_can_recover() {
        let mut d = dialog();
        d.fields[0] = "a".repeat(2047);
        d.replace = false;
        d.input("é");
        assert_eq!(d.fields[0].len(), 2047);
        assert_eq!(d.id, 17);
        d.replace = true;
        d.input("nuevo\r\n\0.txt");
        assert_eq!(d.fields[0], "nuevo.txt");
        assert!(d.error.is_empty());
    }
}

#[cfg(test)]
mod regex_options_tests {
    use super::*;
    #[test]
    fn multiline_error_summary_keeps_reason_visible_and_is_utf8_bounded() {
        let source = "Regex no válida: regex parse error:\n    (\n    ^\nerror: unclosed group";
        let summary = error_line(source);
        assert!(summary.starts_with("Regex no válida:"));
        assert!(summary.ends_with("error: unclosed group"));
        assert!(!summary.chars().any(char::is_control));
        let long = error_line(&"漢\n".repeat(1000));
        assert!(long.len() <= 512 && long.ends_with('…'));
        assert!(std::str::from_utf8(long.as_bytes()).is_ok());
        assert_eq!(error_line(" \t\n"), "");
    }
    #[test]
    fn regex_toggle_keeps_fields_and_is_opt_in() {
        let mut d = Dialog::new(17, PathBuf::from("/fixture"), Arc::from([]), Mode::Rename);
        d.fields[3] = "(?<n>[0-9]+)".into();
        d.fields[4] = "-${n}-".into();
        assert!(!d.options().unwrap().regex);
        let fields = d.fields.clone();
        d.regex = true;
        d.change();
        let options = d.options().unwrap();
        assert!(options.regex);
        assert_eq!(
            options.replace,
            Some((fields[3].clone(), fields[4].clone()))
        );
        assert_eq!(d.id, 0);
        assert!(d.changed);
        assert_eq!(d.fields, fields);
    }
}
