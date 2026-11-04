//! Immutable bounded diagnostic capture; no filesystem/network work or mount authority.
use crate::{
    notice_navigation::{self, Command},
    painter::Painter,
    sftp_executor,
    sftp_state::Form,
};
use std::fmt::{self, Write};
pub const LIMIT: usize = 24 * 1024;
const TAIL: &str = "\n[Informe truncado a24KiB; datos restantes no incluidos.]\n";
struct Bounded {
    text: String,
    truncated: bool,
}
impl Write for Bounded {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.truncated {
            return Err(fmt::Error);
        }
        let remaining = (LIMIT - TAIL.len()).saturating_sub(self.text.len());
        if s.len() <= remaining {
            self.text.push_str(s);
            return Ok(());
        }
        let mut end = remaining;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        self.text.push_str(&s[..end]);
        self.truncated = true;
        Err(fmt::Error)
    }
}
impl Bounded {
    fn field(&mut self, label: &str, value: &str) {
        let _ = write!(self, "{label}: ");
        for c in value.chars() {
            let result = if c == '\n' {
                self.write_char(c)
            } else if c.is_control() {
                write!(self, "\\u{{{:x}}}", c as u32)
            } else {
                self.write_char(c)
            };
            if result.is_err() {
                break;
            }
        }
        let _ = self.write_char('\n');
    }
}
pub struct Report {
    pub id: u64,
    text: String,
    pub truncated: bool,
    ranges: Vec<[u16; 2]>,
    width: i32,
    pub scroll: usize,
    pub visible_rows: usize,
    pub copy_requests: u64,
}
fn yes(value: bool) -> &'static str {
    if value { "sí" } else { "no" }
}
impl Report {
    pub fn capture(id: u64, f: &Form, status: &sftp_executor::Status, busy: bool) -> Self {
        let mut b = Bounded {
            text: String::with_capacity(1024),
            truncated: false,
        };
        let _=b.write_str("Files · Informe SFTP\nCaptura fija; F5 actualiza sólo este informe.\n\nBorrador del formulario (no identifica una conexión activa)\n");
        for (label, value) in [
            "Host/alias",
            "Usuario",
            "Puerto (vacío: SSH config)",
            "Carpeta remota (vacío: inicio)",
        ]
        .into_iter()
        .zip(&f.fields)
        {
            b.field(label, value);
        }
        b.field(
            "Destino revisado vigente",
            yes(f.review.as_ref().is_some_and(|v| v.probe == f.probe)),
        );
        if !f.error.is_empty() {
            b.field("Error del formulario", &f.error);
        }
        let _ = b.write_str("\nHerramientas locales\n");
        match &f.capability {
            None => b.field("Comprobación", "Pendiente; vuelve al formulario y usa F5"),
            Some(Ok(cap)) => {
                b.field("Capacidad", &cap.summary());
                b.field("Disponible para conectar", yes(cap.tools_ready()));
            }
            Some(Err(error)) => b.field("Error de comprobación", error),
        }
        let _ = b.write_str("\nTransporte (última observación, puede cambiar)\n");
        b.field(
            "Estado",
            if status.label().is_empty() {
                "Sin conexión registrada"
            } else {
                status.label()
            },
        );
        b.field("Operación en curso", yes(busy));
        b.field("Apertura verificada en esta observación", yes(status.open));
        b.field("Registro escribible", yes(status.writable));
        if let Some(point) = &status.point {
            let _ = writeln!(
                b,
                "Punto de montaje (representación escapada): {:?}",
                point.as_os_str()
            );
        } else {
            b.field("Punto de montaje", "No informado");
        }
        if !status.diagnostic.is_empty() {
            b.field("Diagnóstico del transporte", &status.diagnostic);
        }
        let _=b.write_str("\nAcciones desde el formulario\nF5: comprobar herramientas locales.\nEnter: revisar borrador; CtrlEnter: conectar explícitamente.\nAltO: comprobar y abrir el montaje existente.\nAltR: reconciliar el registro; AltS: sincronizarlo.\nAltD: solicitar desmontaje normal.\nEste informe no conecta, reconcilia, sincroniza ni desconecta.\n");
        if b.truncated {
            b.text.push_str(TAIL);
        }
        Self {
            id,
            text: b.text,
            truncated: b.truncated,
            ranges: Vec::new(),
            width: 0,
            scroll: 0,
            visible_rows: 1,
            copy_requests: 0,
        }
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn total(&self) -> usize {
        self.ranges.len()
    }
    pub fn line(&self, i: usize) -> Option<&str> {
        self.ranges
            .get(i)
            .map(|[a, b]| &self.text[usize::from(*a)..usize::from(*b)])
    }
    /// Cache byte ranges, not text clones. Width changes preserve the first visible source byte.
    pub fn layout(&mut self, p: &mut dyn Painter, width: i32, rows: usize) {
        let width = width.max(1);
        self.visible_rows = rows.max(1);
        if self.width != width || self.ranges.is_empty() {
            let anchor = self.ranges.get(self.scroll).map_or(0, |r| r[0]);
            self.ranges.clear();
            self.width = width;
            let mut start = 0usize;
            let mut used = 0f32;
            let mut last_break: Option<(usize, f32)> = None;
            for (at, c) in self.text.char_indices() {
                if c == '\n' {
                    self.ranges.push([start as u16, at as u16]);
                    start = at + 1;
                    used = 0.;
                    last_break = None;
                    continue;
                }
                let mut buf = [0; 4];
                let advance = p.text_width(c.encode_utf8(&mut buf), 12, true).max(0.);
                if used + advance > width as f32 && start < at {
                    if let Some((after, measured)) =
                        last_break.take().filter(|(after, _)| *after > start)
                    {
                        self.ranges.push([start as u16, after as u16]);
                        start = after;
                        used -= measured;
                    }
                    if used + advance > width as f32 && start < at {
                        self.ranges.push([start as u16, at as u16]);
                        start = at;
                        used = 0.;
                    }
                }
                used += advance;
                if c.is_whitespace() {
                    last_break = Some((at + c.len_utf8(), used));
                }
            }
            self.ranges.push([start as u16, self.text.len() as u16]);
            self.scroll = self
                .ranges
                .partition_point(|r| r[0] <= anchor)
                .saturating_sub(1);
        }
        self.navigate(Command::Rows(0));
    }
    pub fn ranges_for_paint(&self) -> impl Iterator<Item = &str> {
        (self.scroll
            ..self
                .scroll
                .saturating_add(self.visible_rows)
                .min(self.total()))
            .filter_map(|i| self.line(i))
    }
    pub fn navigate(&mut self, command: Command) {
        self.scroll =
            notice_navigation::navigate(self.scroll, self.total(), self.visible_rows, command);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::painter::{Image, Rect};
    use files_core::theme::Color;
    struct Paint;
    impl Painter for Paint {
        fn rect(&mut self, _: Rect, _: Color) {}
        fn rounded_rect(&mut self, _: Rect, _: i32, _: Color) {}
        fn text(&mut self, _: Rect, _: &str, _: i32, _: Color, _: bool, _: bool) {}
        fn image(&mut self, _: Rect, _: &Image<'_>) {}
        fn clip(&mut self, _: Rect) {}
        fn unclip(&mut self) {}
    }
    fn capture(diagnostic: &str) -> Report {
        let mut state = crate::sftp_state::State::default();
        state.open();
        let f = state.form.as_mut().unwrap();
        f.fields = [
            "Draft-Alias".into(),
            "user".into(),
            "0022".into(),
            "/ñ/%41/$()".into(),
        ];
        f.review();
        Report::capture(
            1,
            f,
            &sftp_executor::Status {
                phase: "Uncertain".into(),
                diagnostic: diagnostic.into(),
                ..Default::default()
            },
            false,
        )
    }
    #[test]
    fn snapshot_distinguishes_draft_status_and_escapes_controls_without_losing_lf() {
        let r = capture("alpha\nbeta\t\x1b\0END");
        assert!(!r.truncated);
        assert!(
            r.text()
                .contains("Borrador del formulario (no identifica una conexión activa)")
        );
        assert!(r.text().contains("0022"));
        assert!(r.text().contains("/ñ/%41/$()"));
        assert!(r.text().contains("Estado incierto"));
        assert!(r.text().contains("alpha\nbeta\\u{9}\\u{1b}\\u{0}END"));
        assert!(!r.text().contains('\x1b'));
        assert!(!r.text().contains('\0'));
    }
    #[test]
    fn oversized_input_has_explicit_utf8_safe_tail_and_bounded_capture() {
        let r = capture(&"漢".repeat(LIMIT));
        assert!(r.truncated);
        assert!(r.text().len() <= LIMIT);
        assert!(r.text().ends_with(TAIL));
        assert!(std::str::from_utf8(r.text().as_bytes()).is_ok());
    }
    #[test]
    fn wrapping_is_lossless_for_long_unicode_and_newlines_and_end_is_full_page() {
        let mut r = capture(&format!("{}\nFINAL", "ñ🙂漢".repeat(800)));
        let mut p = Paint;
        r.layout(&mut p, 200, 8);
        let mut at = 0;
        for [start, end] in &r.ranges {
            let (a, b) = (usize::from(*start), usize::from(*end));
            assert!(a >= at && a - at <= 1);
            if a > at {
                assert_eq!(&r.text[at..a], "\n");
            }
            assert!(r.text.is_char_boundary(a) && r.text.is_char_boundary(b));
            at = b;
        }
        assert_eq!(at, r.text.len());
        r.navigate(Command::End);
        assert_eq!(r.scroll + 8, r.total());
        assert_eq!(r.ranges_for_paint().count(), 8);
        r.navigate(Command::Pages(-1));
        assert!(r.scroll + 8 < r.total());
        r.navigate(Command::Start);
        assert_eq!(r.scroll, 0);
    }
    #[test]
    fn width_change_preserves_source_anchor_and_height_clamps_without_rebuilding_text() {
        let mut r = capture(&"word ñ ".repeat(600));
        let mut p = Paint;
        r.layout(&mut p, 200, 6);
        r.navigate(Command::Pages(3));
        let anchor = r.ranges[r.scroll][0];
        let ptr = r.text.as_ptr();
        r.layout(&mut p, 360, 6);
        let range = r.ranges[r.scroll];
        assert!(range[0] <= anchor && anchor <= range[1]);
        assert_eq!(r.text.as_ptr(), ptr);
        r.navigate(Command::End);
        r.layout(&mut p, 360, 10000);
        assert_eq!(r.scroll, 0);
    }
    #[test]
    fn wraps_at_spaces_without_dropping_them_and_falls_back_for_long_tokens() {
        let mut r = capture("");
        r.text = "alpha beta gamma".into();
        let mut p = Paint;
        r.layout(&mut p, 75, 8);
        assert_eq!(r.line(0), Some("alpha "));
        assert_eq!(r.line(1), Some("beta gamma"));
        assert_eq!(r.ranges_for_paint().collect::<String>(), "alpha beta gamma");
        r.text = "x".repeat(100);
        r.width = 0;
        r.layout(&mut p, 75, 20);
        assert!(r.total() > 1);
        assert_eq!(r.ranges_for_paint().collect::<String>(), "x".repeat(100));
    }
    #[test]
    fn raw_mountpoint_is_escaped_and_does_not_become_lossy_replacement_path() {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let mut s = crate::sftp_state::State::default();
            s.open();
            let r = Report::capture(
                1,
                s.form.as_ref().unwrap(),
                &sftp_executor::Status {
                    point: Some(std::ffi::OsString::from_vec(b"/mount-\xff".to_vec()).into()),
                    ..Default::default()
                },
                false,
            );
            assert!(r.text().contains("\\xFF") || r.text().contains("\\xff"));
            assert!(!r.text().contains('�'));
        }
    }
}
