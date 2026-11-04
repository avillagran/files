//! Visible text lines only; byte-safe selection, with shaped width from Painter.
use crate::{
    painter::{Painter, Rect},
    text_document::TextDocument,
};
use files_core::theme::Palette;
const SIZE: i32 = 13;
const LINE: i32 = 23;
const MAX_PAINT_BYTES: usize = 4096;
#[derive(Default)]
pub struct Viewport {
    pub top: usize,
    pub horizontal: f32,
    pub dragging: bool,
    pub body: Rect,
    pub selection_anchor: usize,
}
fn visible_prefix(text: &str) -> &str {
    let mut n = text.len().min(MAX_PAINT_BYTES);
    while !text.is_char_boundary(n) {
        n -= 1;
    }
    &text[..n]
}
/// One bounded visible-line transformation; source bytes are never rewritten.
struct RenderLine {
    display: String,
    boundaries: Vec<(usize, usize)>,
    source_len: usize,
}
impl RenderLine {
    fn new(source: &str) -> Self {
        let source = visible_prefix(source);
        let mut display = String::new();
        let mut boundaries = vec![(0, 0)];
        let mut column = 0usize;
        for (offset, ch) in source.char_indices() {
            if ch == '\t' {
                let spaces = 8 - column % 8;
                display.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            } else if ch.is_ascii_control() {
                display.push('^');
                display.push((ch as u8 ^ 0x40) as char);
                column += 2;
            } else {
                display.push(ch);
                column += 1;
            }
            boundaries.push((offset + ch.len_utf8(), display.len()));
        }
        Self {
            display,
            boundaries,
            source_len: source.len(),
        }
    }
    fn display_offset(&self, source: usize) -> usize {
        let index = self
            .boundaries
            .partition_point(|(offset, _)| *offset <= source.min(self.source_len))
            .saturating_sub(1);
        self.boundaries[index].1
    }
}
impl Viewport {
    pub fn scroll(&mut self, delta: i32, doc: &TextDocument) {
        self.top = (self.top as i64 + delta as i64 * 3)
            .clamp(0, doc.line_count().saturating_sub(1) as i64) as usize;
    }
    pub fn reveal(&mut self, doc: &TextDocument) {
        let (line, _) = doc.position(doc.caret());
        let rows = (self.body.h / LINE).max(1) as usize;
        if line < self.top {
            self.top = line;
        }
        if line >= self.top + rows {
            self.top = line + 1 - rows;
        }
    }
    pub fn point_for_offset(
        &self,
        p: &mut dyn Painter,
        doc: &TextDocument,
        offset: usize,
    ) -> (i32, i32) {
        let (line, _) = doc.position(offset);
        let rendered = RenderLine::new(doc.line(line).unwrap_or_default());
        let start = doc.line_range(line).expect("existing line").start;
        let advance = p.text_width(
            &rendered.display[..rendered.display_offset(offset.saturating_sub(start))],
            SIZE,
            true,
        );
        (
            self.body.x + 58 + (advance - self.horizontal).round() as i32,
            self.body.y + (line.saturating_sub(self.top)) as i32 * LINE + LINE / 2,
        )
    }
    pub fn offset_at(&self, p: &mut dyn Painter, doc: &TextDocument, x: i32, y: i32) -> usize {
        let line =
            (self.top + ((y - self.body.y).max(0) / LINE) as usize).min(doc.line_count() - 1);
        let source = doc.line(line).unwrap_or_default();
        let rendered = RenderLine::new(source);
        let hit = (x - self.body.x - 58) as f32 + self.horizontal;
        let mut lo = 0;
        let mut hi = rendered.boundaries.len() - 1;
        while lo < hi {
            let mid = (lo + hi) / 2;
            let start = p.text_width(&rendered.display[..rendered.boundaries[mid].1], SIZE, true);
            let end = p.text_width(
                &rendered.display[..rendered.boundaries[mid + 1].1],
                SIZE,
                true,
            );
            if hit > (start + end) / 2.0 {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        doc.line_range(line).expect("existing line").start + rendered.boundaries[lo].0
    }
    pub fn paint(&mut self, p: &mut dyn Painter, doc: &TextDocument, body: Rect, c: &Palette) {
        self.body = body;
        self.top = self.top.min(doc.line_count() - 1);
        p.clip(body);
        let selection = doc.selection_range();
        for row in 0..((body.h / LINE).max(0) + 1) as usize {
            let line = self.top + row;
            let Some(range) = doc.line_range(line) else {
                break;
            };
            let rendered = RenderLine::new(doc.line(line).unwrap_or_default());
            let text = &rendered.display;
            let y = body.y + row as i32 * LINE;
            p.text(
                Rect::new(body.x, y, 44, LINE),
                &(line + 1).to_string(),
                10,
                c.dark_foreground,
                true,
                true,
            );
            let tx = body.x + 58 - self.horizontal as i32;
            let end = range.start + rendered.source_len;
            let text_area = Rect::new(body.x + 52, y, (body.w - 52).max(0), LINE);
            p.clip(text_area);
            if !selection.is_empty()
                && selection.start < range.end + 1
                && selection.end > range.start
            {
                let start = rendered.display_offset(selection.start.saturating_sub(range.start));
                let stop = rendered.display_offset(selection.end.saturating_sub(range.start));
                // Source offsets are already safe UTF-8 boundaries.
                let left = p.text_width(&text[..start], SIZE, true) as i32;
                let right = p.text_width(&text[..stop], SIZE, true) as i32;
                p.rect(
                    Rect::new(tx + left, y, (right - left).max(2), LINE),
                    c.selection,
                );
            }
            let width = p.text_width(text, SIZE, true).ceil() as i32 + 2;
            p.literal_text(Rect::new(tx, y, width, LINE), text, SIZE, c.foreground);
            if doc.caret() >= range.start && doc.caret() <= end {
                let caret = p.text_width(
                    &text[..rendered.display_offset(doc.caret() - range.start)],
                    SIZE,
                    true,
                ) as i32;
                p.rect(Rect::new(tx + caret, y + 3, 1, LINE - 6), c.accent);
            }
            p.unclip();
            if rendered.source_len != range.len() {
                p.text(
                    Rect::new(body.x + body.w - 236, y, 230, LINE),
                    "[línea larga: copia completa]",
                    10,
                    c.dark_foreground,
                    true,
                    true,
                );
            }
        }
        p.unclip();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Width;
    impl Painter for Width {
        fn rect(&mut self, _: Rect, _: files_core::theme::Color) {}
        fn rounded_rect(&mut self, _: Rect, _: i32, _: files_core::theme::Color) {}
        fn text(
            &mut self,
            _: Rect,
            _: &str,
            _: i32,
            _: files_core::theme::Color,
            _: bool,
            _: bool,
        ) {
        }
        fn text_width(&mut self, s: &str, _: i32, _: bool) -> f32 {
            s.chars().map(|c| if c == '漢' { 16.0 } else { 8.0 }).sum()
        }
        fn image(&mut self, _: Rect, _: &crate::painter::Image<'_>) {}
        fn clip(&mut self, _: Rect) {}
        fn unclip(&mut self) {}
    }
    #[test]
    fn pointer_uses_measured_width_and_never_splits_utf8() {
        let d = TextDocument::new("a漢b\r\nnext".into()).unwrap();
        let v = Viewport {
            body: Rect::new(10, 20, 300, 100),
            ..Default::default()
        };
        assert_eq!(v.offset_at(&mut Width, &d, 68, 20), 0);
        assert_eq!(v.offset_at(&mut Width, &d, 80, 20), 1);
        assert_eq!(v.offset_at(&mut Width, &d, 90, 20), 4);
        assert_eq!(v.offset_at(&mut Width, &d, 200, 43), 11);
    }
    #[test]
    fn visible_text_is_bounded_without_changing_document_or_copy() {
        let mut d = TextDocument::new(format!("{}😀\nlast", "á".repeat(4096))).unwrap();
        let prefix = visible_prefix(d.line(0).unwrap());
        assert_eq!(prefix.len(), 4096);
        assert!(prefix.is_char_boundary(prefix.len()));
        let mut v = Viewport::default();
        v.paint(
            &mut Width,
            &d,
            Rect::new(0, 0, 300, 50),
            &Palette::default(),
        );
        d.select_all();
        assert_eq!(d.selected_text(), d.text());
        v.scroll(i32::MAX, &d);
        assert_eq!(v.top, 1);
    }
    #[derive(Default)]
    struct Recorder {
        rectangles: Vec<(Rect, files_core::theme::Color)>,
        clips: Vec<Rect>,
        literal_clips: Vec<Rect>,
        literals: Vec<String>,
    }
    impl Painter for Recorder {
        fn rect(&mut self, r: Rect, c: files_core::theme::Color) {
            self.rectangles.push((r, c));
        }
        fn rounded_rect(&mut self, _: Rect, _: i32, _: files_core::theme::Color) {}
        fn text(
            &mut self,
            _: Rect,
            _: &str,
            _: i32,
            _: files_core::theme::Color,
            _: bool,
            _: bool,
        ) {
        }
        fn literal_text(&mut self, _: Rect, text: &str, _: i32, _: files_core::theme::Color) {
            self.literals.push(text.into());
            self.literal_clips.push(*self.clips.last().unwrap());
        }
        fn text_width(&mut self, s: &str, _: i32, _: bool) -> f32 {
            Width.text_width(s, 0, true)
        }
        fn image(&mut self, _: Rect, _: &crate::painter::Image<'_>) {}
        fn clip(&mut self, r: Rect) {
            self.clips.push(r);
        }
        fn unclip(&mut self) {
            self.clips.pop().unwrap();
        }
    }
    #[test]
    fn empty_selection_has_only_caret_and_horizontal_text_cannot_cover_gutter() {
        let mut d = TextDocument::new("abc".into()).unwrap();
        d.move_to(1, false);
        let palette = Palette::default();
        let mut p = Recorder::default();
        let mut v = Viewport {
            horizontal: 100.0,
            ..Default::default()
        };
        v.paint(&mut p, &d, Rect::new(10, 20, 300, 100), &palette);
        assert_eq!(
            p.rectangles
                .iter()
                .filter(|(_, c)| *c == palette.selection)
                .count(),
            0
        );
        assert_eq!(
            p.rectangles
                .iter()
                .filter(|(_, c)| *c == palette.accent)
                .count(),
            1
        );
        assert!(p.literal_clips.iter().all(|r| r.x >= 62));
        assert!(p.clips.is_empty());
    }
    #[test]
    fn tab_control_render_hit_selection_and_copy_use_one_map() {
        let mut d = TextDocument::new("a\tb\r\n@x\u{7}".into()).unwrap();
        let mut p = Recorder::default();
        let palette = Palette::default();
        let mut v = Viewport::default();
        v.paint(&mut p, &d, Rect::new(10, 20, 300, 100), &palette);
        assert_eq!(p.literals[0], "a       b");
        assert_eq!(p.literals[1], "@x^G");
        let start = v.offset_at(&mut p, &d, 133, 20);
        let end = v.offset_at(&mut p, &d, 141, 20);
        assert_eq!((start, end), (2, 3));
        d.select(start, end);
        assert_eq!(d.selected_text(), "b");
        p.rectangles.clear();
        v.paint(&mut p, &d, Rect::new(10, 20, 300, 100), &palette);
        let (highlight, _) = p
            .rectangles
            .iter()
            .find(|(_, c)| *c == palette.selection)
            .unwrap();
        assert_eq!((highlight.x, highlight.w), (132, 8));
        d.select_all();
        assert_eq!(d.selected_text(), "a\tb\r\n@x\u{7}");
    }
}
