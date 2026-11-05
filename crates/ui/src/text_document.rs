//! Bounded, immutable source text and borrowed line/selection slices for preview.
//! Offsets are UTF-8 byte offsets; columns are Unicode scalar counts, not pixels
//! or grapheme clusters. The renderer must map shaped glyph positions separately.
use std::ops::Range;

pub const MAX_TEXT_BYTES: usize = 384 * 1024;
pub const MAX_LINES: usize = 2001; // 2000 LF delimiters can leave a final empty line.

pub struct TextDocument {
    text: String,
    starts: Vec<usize>,
    anchor: usize,
    caret: usize,
}
impl TextDocument {
    /// Takes ownership without cloning the source. Refuses excess input instead
    /// of silently truncating: truncation belongs to the helper and its UI label.
    pub fn new(text: String) -> Result<Self, &'static str> {
        if text.len() > MAX_TEXT_BYTES {
            return Err("Texto fuera del límite de vista previa");
        }
        let mut starts = Vec::new();
        starts.push(0);
        for (index, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                if starts.len() == MAX_LINES {
                    return Err("Demasiadas líneas para la vista previa");
                }
                starts.push(index + 1);
            }
        }
        Ok(Self {
            text,
            starts,
            anchor: 0,
            caret: 0,
        })
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn line_count(&self) -> usize {
        self.starts.len()
    }
    /// Content range excludes CRLF/LF; selection still preserves original bytes.
    pub fn line_range(&self, line: usize) -> Option<Range<usize>> {
        let start = *self.starts.get(line)?;
        let mut end = self
            .starts
            .get(line + 1)
            .copied()
            .unwrap_or(self.text.len());
        if end > start && self.text.as_bytes()[end - 1] == b'\n' {
            end -= 1;
            if end > start && self.text.as_bytes()[end - 1] == b'\r' {
                end -= 1;
            }
        }
        Some(start..end)
    }
    pub fn line(&self, line: usize) -> Option<&str> {
        self.line_range(line).map(|range| &self.text[range])
    }
    /// Clamp a mouse/keyboard-derived byte offset without splitting UTF-8 or CRLF.
    pub fn boundary(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        if offset > 0
            && self.text.as_bytes().get(offset) == Some(&b'\n')
            && self.text.as_bytes()[offset - 1] == b'\r'
        {
            offset -= 1;
        }
        offset
    }
    /// Convert a logical line/scalar column to a safe offset, clamped at line end.
    pub fn offset(&self, line: usize, column: usize) -> usize {
        let line = line.min(self.starts.len() - 1);
        let range = self.line_range(line).expect("clamped line");
        range.start
            + self.text[range.clone()]
                .char_indices()
                .nth(column)
                .map_or(range.len(), |(index, _)| index)
    }
    pub fn position(&self, offset: usize) -> (usize, usize) {
        let offset = self.boundary(offset);
        let line = self
            .starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1);
        let range = self.line_range(line).expect("existing line");
        (
            line,
            self.text[range.start..offset.min(range.end)]
                .chars()
                .count(),
        )
    }
    pub fn select(&mut self, anchor: usize, caret: usize) {
        self.anchor = self.boundary(anchor);
        self.caret = self.boundary(caret);
    }
    /// Double click: the run of word characters (letters, digits, `_`), of
    /// blanks or of punctuation around `offset`, never crossing a line end.
    pub fn word_range(&self, offset: usize) -> Range<usize> {
        fn class(c: char) -> u8 {
            if c.is_alphanumeric() || c == '_' {
                0
            } else if c.is_whitespace() {
                1
            } else {
                2
            }
        }
        let offset = self.boundary(offset);
        let line = self
            .line_range(self.position(offset).0)
            .expect("existing line");
        let text = &self.text[line.clone()];
        let at = offset.clamp(line.start, line.end) - line.start;
        let Some(target) = text[at..]
            .chars()
            .next()
            .or_else(|| text[..at].chars().next_back())
        else {
            return offset..offset;
        };
        let kind = class(target);
        let start = text[..at]
            .char_indices()
            .rev()
            .take_while(|(_, c)| class(*c) == kind)
            .last()
            .map_or(at, |(i, _)| i);
        let end = text[at..]
            .char_indices()
            .find(|(_, c)| class(*c) != kind)
            .map_or(text.len(), |(i, _)| at + i);
        line.start + start..line.start + end
    }
    /// Triple click: the whole line with its original line ending.
    pub fn line_span(&self, offset: usize) -> Range<usize> {
        let line = self.position(offset).0;
        let start = self.starts[line];
        start..self.starts.get(line + 1).copied().unwrap_or(self.text.len())
    }
    /// A pointer press with FLTK's extra-click count (0 single, 1 double,
    /// 2+ triple). Returns the anchor a following drag extends from.
    pub fn click(&mut self, offset: usize, extend: bool, clicks: i32) -> usize {
        let range = match clicks {
            1 => self.word_range(offset),
            2.. => self.line_span(offset),
            _ => {
                self.move_to(offset, extend);
                let range = self.selection_range();
                return if self.caret == range.start {
                    range.end
                } else {
                    range.start
                };
            }
        };
        self.select(range.start, range.end);
        range.start
    }
    pub fn select_all(&mut self) {
        self.select(0, self.text.len());
    }
    pub fn caret(&self) -> usize {
        self.caret
    }
    pub fn selection_range(&self) -> Range<usize> {
        self.anchor.min(self.caret)..self.anchor.max(self.caret)
    }
    pub fn selected_text(&self) -> &str {
        &self.text[self.selection_range()]
    }
    pub fn move_to(&mut self, offset: usize, extend: bool) {
        let offset = self.boundary(offset);
        if !extend {
            self.anchor = offset;
        }
        self.caret = offset;
    }
    /// Move by one Unicode scalar, treating CRLF as a single delimiter.
    /// Without Shift, an existing selection collapses in the requested direction.
    pub fn move_horizontal(&mut self, right: bool, extend: bool) {
        let selected = self.selection_range();
        if !extend && !selected.is_empty() {
            self.move_to(if right { selected.end } else { selected.start }, false);
            return;
        }
        let next = if right {
            if self.text[self.caret..].starts_with("\r\n") {
                self.caret + 2
            } else {
                self.caret
                    + self.text[self.caret..]
                        .chars()
                        .next()
                        .map_or(0, char::len_utf8)
            }
        } else if self.text[..self.caret].ends_with("\r\n") {
            self.caret - 2
        } else {
            self.caret
                - self.text[..self.caret]
                    .chars()
                    .next_back()
                    .map_or(0, char::len_utf8)
        };
        self.move_to(next, extend);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn double_and_triple_click_select_word_and_line() {
        let mut doc = TextDocument::new("hola mundo_2, área\r\nsegunda línea\nfin".into()).unwrap();
        assert_eq!(doc.click(7, false, 1), 5);
        assert_eq!(doc.selected_text(), "mundo_2");
        doc.click(16, false, 1);
        assert_eq!(doc.selected_text(), "área");
        // At the end of a line the word before it is chosen, not the CRLF.
        doc.click(doc.line_range(0).unwrap().end, false, 1);
        assert_eq!(doc.selected_text(), "área");
        doc.click(12, false, 1);
        assert_eq!(doc.selected_text(), ",");
        doc.click(4, false, 1);
        assert_eq!(doc.selected_text(), " ");
        doc.click(25, false, 2);
        assert_eq!(doc.selected_text(), "segunda línea\n");
        doc.click(0, false, 3);
        assert_eq!(doc.selected_text(), "hola mundo_2, área\r\n");
        let end = doc.text().len();
        doc.click(end, false, 2);
        assert_eq!(doc.selected_text(), "fin");
        // A single click collapses; Shift+click extends from the old anchor.
        assert_eq!(doc.click(2, false, 0), 2);
        assert!(doc.selected_text().is_empty());
        doc.click(10, true, 0);
        assert_eq!(doc.selected_text(), "la mundo");
        assert!(TextDocument::new(String::new()).unwrap().word_range(0).is_empty());
    }
    #[test]
    fn lines_borrow_source_and_copy_preserves_original_endings() {
        let mut doc = TextDocument::new("área\r\n漢字\nlast".into()).unwrap();
        assert_eq!(doc.line_count(), 3);
        assert_eq!(doc.line(0), Some("área"));
        assert_eq!(doc.line(1), Some("漢字"));
        assert_eq!(doc.line(3), None);
        assert_eq!(
            doc.line(1).unwrap().as_ptr(),
            doc.text().as_ptr().wrapping_add(7)
        );
        doc.select(doc.offset(0, 1), doc.offset(2, 2));
        assert_eq!(doc.selected_text(), "rea\r\n漢字\nla");
        doc.select_all();
        assert_eq!(doc.selected_text(), doc.text());
    }
    #[test]
    fn byte_offsets_inside_unicode_or_crlf_are_safe() {
        let mut doc = TextDocument::new("á😀\r\nx".into()).unwrap();
        for offset in 0..=doc.text().len() + 4 {
            let safe = doc.boundary(offset);
            assert!(doc.text().is_char_boundary(safe));
            assert_ne!(safe, 7); // between CR and LF
            let (line, column) = doc.position(offset);
            assert_eq!(doc.offset(line, column), safe);
            doc.select(usize::MAX, offset);
            assert!(std::str::from_utf8(doc.selected_text().as_bytes()).is_ok());
        }
        assert_eq!(doc.offset(0, usize::MAX), 6);
        assert_eq!(doc.offset(usize::MAX, usize::MAX), 9);
    }
    #[test]
    fn horizontal_selection_crosses_crlf_without_splitting() {
        let mut doc = TextDocument::new("á\r\n😀".into()).unwrap();
        for expected in [2, 4, 8, 8] {
            doc.move_horizontal(true, true);
            assert_eq!(doc.caret(), expected);
        }
        assert_eq!(doc.selected_text(), "á\r\n😀");
        doc.move_horizontal(false, false);
        assert_eq!(doc.caret(), 0);
        assert!(doc.selected_text().is_empty());
        doc.move_to(8, false);
        for expected in [4, 2, 0, 0] {
            doc.move_horizontal(false, true);
            assert_eq!(doc.caret(), expected);
        }
        doc.move_horizontal(true, false);
        assert_eq!(doc.caret(), 8);
    }
    #[test]
    fn empty_final_lines_and_budgets_are_explicit() {
        let empty = TextDocument::new(String::new()).unwrap();
        assert_eq!(empty.line(0), Some(""));
        assert_eq!(empty.offset(99, 99), 0);
        let lines = TextDocument::new("\n".repeat(2000)).unwrap();
        assert_eq!(lines.line_count(), 2001);
        assert_eq!(lines.line(2000), Some(""));
        assert!(TextDocument::new("\n".repeat(2001)).is_err());
        assert!(TextDocument::new("x".repeat(MAX_TEXT_BYTES)).is_ok());
        assert!(TextDocument::new("x".repeat(MAX_TEXT_BYTES + 1)).is_err());
    }
}
