//! The rich text document: a flat list of blocks, each one a string with
//! styled runs. Blocks are plain data (`Send`), so a long transcript can be
//! parsed in a background [`Task`](crate::task::Task) and handed to the UI.
//!
//! Every block keeps its whole text in one `String`; styles are byte
//! ranges over it ([`Run`]), so the plain text of a block, of a selection or
//! of the whole document is a slice, never a rebuild. Nesting (lists, block
//! quotes) is a depth on each block, not a tree: appending is a `push`.

use crate::theme::{Rgb, Theme};
use std::ops::Range;
use std::sync::Arc;

/// A color from the theme palette, resolved when drawing: a live theme
/// change re-colors the text without parsing or laying it out again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorRole {
    Foreground,
    /// `bright_foreground` (emphasis).
    Bright,
    /// `darker(fg, 1.4)`: secondary text.
    Dim,
    /// `darker(fg, 1.5)`: descriptions, comments.
    Faint,
    Accent,
    Urgent,
    Red,
    Green,
    Yellow,
    Blue,
    Cyan,
    Magenta,
    Background,
    DarkerBackground,
    LighterBackground,
}

impl ColorRole {
    /// Every role, in declaration order (compact storage indexes into it).
    pub(crate) const ALL: [ColorRole; 15] = [
        ColorRole::Foreground,
        ColorRole::Bright,
        ColorRole::Dim,
        ColorRole::Faint,
        ColorRole::Accent,
        ColorRole::Urgent,
        ColorRole::Red,
        ColorRole::Green,
        ColorRole::Yellow,
        ColorRole::Blue,
        ColorRole::Cyan,
        ColorRole::Magenta,
        ColorRole::Background,
        ColorRole::DarkerBackground,
        ColorRole::LighterBackground,
    ];

    pub fn rgb(self, t: &Theme) -> Rgb {
        let p = &t.palette;
        match self {
            ColorRole::Foreground => p.foreground,
            ColorRole::Bright => p.bright_foreground,
            ColorRole::Dim => p.foreground.darker(1.4),
            ColorRole::Faint => p.foreground.darker(1.5),
            ColorRole::Accent => p.accent,
            ColorRole::Urgent => p.urgent,
            ColorRole::Red => p.red,
            ColorRole::Green => p.green,
            ColorRole::Yellow => p.yellow,
            ColorRole::Blue => p.blue,
            ColorRole::Cyan => p.cyan,
            ColorRole::Magenta => p.magenta,
            ColorRole::Background => p.background,
            ColorRole::DarkerBackground => p.darker_background,
            ColorRole::LighterBackground => p.lighter_background,
        }
    }
}

/// Inline style of a run of text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    /// Inline code: monospace on a chip.
    pub code: bool,
    /// Link target; clicks and Enter go to `RichText::on_link`.
    pub link: Option<Arc<str>>,
    /// Text color role (links default to the accent).
    pub color: Option<ColorRole>,
}

impl Style {
    pub fn plain() -> Style {
        Style::default()
    }
    pub fn bold(mut self) -> Style {
        self.bold = true;
        self
    }
    pub fn italic(mut self) -> Style {
        self.italic = true;
        self
    }
    pub fn strike(mut self) -> Style {
        self.strike = true;
        self
    }
    pub fn code(mut self) -> Style {
        self.code = true;
        self
    }
    pub fn link(mut self, url: &str) -> Style {
        self.link = Some(Arc::from(url));
        self
    }
    pub fn color(mut self, role: ColorRole) -> Style {
        self.color = Some(role);
        self
    }
}

/// A styled byte range of a block's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub range: Range<usize>,
    pub style: Style,
}

/// Text with styled runs. The runs cover the text in order, without gaps;
/// neighbours never share a style (pushing the same style extends the run).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inline {
    text: String,
    runs: Vec<Run>,
}

impl Inline {
    pub fn new() -> Inline {
        Inline::default()
    }

    /// Text and runs that already cover it in order (compact storage).
    pub(crate) fn from_parts(text: String, runs: Vec<Run>) -> Inline {
        debug_assert!(runs.last().map_or(text.is_empty(), |r| r.range.end == text.len()));
        Inline { text, runs }
    }

    pub fn plain(text: &str) -> Inline {
        let mut i = Inline::new();
        i.push(text, &Style::plain());
        i
    }

    /// Appends `text` in `style`. NUL becomes U+FFFD and CR is dropped
    /// (FLTK draws C strings).
    pub fn push(&mut self, text: &str, style: &Style) {
        if text.is_empty() {
            return;
        }
        let start = self.text.len();
        if text.bytes().any(|b| b == 0 || b == b'\r') {
            for c in text.chars() {
                match c {
                    '\0' => self.text.push('\u{fffd}'),
                    '\r' => {}
                    c => self.text.push(c),
                }
            }
        } else {
            self.text.push_str(text);
        }
        let end = self.text.len();
        if end == start {
            return;
        }
        match self.runs.last_mut() {
            Some(r) if r.style == *style => r.range.end = end,
            _ => self.runs.push(Run {
                range: start..end,
                style: style.clone(),
            }),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn runs(&self) -> &[Run] {
        &self.runs
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Index of the run that holds byte `at`.
    pub fn run_at(&self, at: usize) -> Option<usize> {
        let i = self.runs.partition_point(|r| r.range.end <= at);
        (i < self.runs.len()).then_some(i)
    }

    /// Removes `n` bytes from the end (must be a char boundary).
    pub(crate) fn truncate(&mut self, len: usize) {
        if len >= self.text.len() {
            return;
        }
        self.text.truncate(len);
        while self.runs.last().is_some_and(|r| r.range.start >= len) {
            self.runs.pop();
        }
        if let Some(r) = self.runs.last_mut() {
            r.range.end = r.range.end.min(len);
        }
    }

    /// Frees spare capacity (finished blocks of a long transcript).
    pub(crate) fn shrink(&mut self) {
        self.text.shrink_to_fit();
        self.runs.shrink_to_fit();
    }

    /// Replaces the runs (they must cover the text in order).
    pub(crate) fn set_runs(&mut self, runs: Vec<Run>) {
        debug_assert!(runs.first().map_or(true, |r| r.range.start == 0));
        debug_assert!(runs.last().map_or(true, |r| r.range.end == self.text.len()));
        self.runs = runs;
    }
}

/// List item marker, drawn before the first line of its block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMarker {
    Bullet,
    Ordered(u64),
    /// GFM task list item, checked or not.
    Task(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColumnAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// A table. The block text holds the cells: tab between cells, newline
/// between rows; `rows` are the byte ranges of each cell in that text.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Table {
    pub align: Vec<ColumnAlign>,
    pub header_rows: usize,
    pub rows: Vec<Vec<Range<usize>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    /// Heading level 1–6.
    Heading(u8),
    /// Code block (monospace, on a theme surface). `lang` is the first word
    /// of the fence info string ("rust", "diff"…), empty when absent.
    Code {
        lang: String,
    },
    /// Thematic break.
    Rule,
    Table(Box<Table>),
}

/// One block of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    pub content: Inline,
    /// List nesting depth (1 = an item of a top-level list or a paragraph
    /// inside one).
    pub indent: u8,
    /// Block quote depth.
    pub quote: u8,
    /// Color of the quote bar (GFM alerts: note, tip, warning…).
    pub quote_tone: Option<ColorRole>,
    /// Set on the first block of a list item.
    pub marker: Option<ListMarker>,
}

impl Block {
    pub fn new(kind: BlockKind) -> Block {
        Block {
            kind,
            content: Inline::new(),
            indent: 0,
            quote: 0,
            quote_tone: None,
            marker: None,
        }
    }

    pub fn paragraph(text: &str) -> Block {
        Block::new(BlockKind::Paragraph).push(text, &Style::plain())
    }

    pub fn heading(level: u8, text: &str) -> Block {
        Block::new(BlockKind::Heading(level.clamp(1, 6))).push(text, &Style::plain())
    }

    pub fn code(lang: &str, code: &str) -> Block {
        Block::new(BlockKind::Code { lang: lang.into() }).push(code, &Style::plain())
    }

    pub fn rule() -> Block {
        Block::new(BlockKind::Rule)
    }

    /// A table from cell texts (first row is the header when `header`).
    pub fn table(align: &[ColumnAlign], rows: &[&[&str]], header: bool) -> Block {
        let mut content = Inline::new();
        let mut t = Table {
            align: align.to_vec(),
            header_rows: usize::from(header && !rows.is_empty()),
            rows: Vec::new(),
        };
        for (ri, row) in rows.iter().enumerate() {
            if ri > 0 {
                content.push("\n", &Style::plain());
            }
            let mut cells = Vec::new();
            for (ci, cell) in row.iter().enumerate() {
                if ci > 0 {
                    content.push("\t", &Style::plain());
                }
                let s = content.len();
                content.push(&cell.replace(['\t', '\n'], " "), &Style::plain());
                cells.push(s..content.len());
            }
            t.rows.push(cells);
        }
        let mut b = Block::new(BlockKind::Table(Box::new(t)));
        b.content = content;
        b
    }

    /// Appends styled text.
    pub fn push(mut self, text: &str, style: &Style) -> Block {
        self.content.push(text, style);
        self
    }

    /// Makes this the first block of a list item at `depth` (1 = top level).
    pub fn item(mut self, marker: ListMarker, depth: u8) -> Block {
        self.marker = Some(marker);
        self.indent = depth.max(1);
        self
    }

    pub fn quoted(mut self, depth: u8) -> Block {
        self.quote = depth;
        self
    }

    /// The block's plain text (tables: cells split by tab, rows by newline).
    pub fn text(&self) -> &str {
        self.content.text()
    }

    pub fn is_code(&self) -> bool {
        matches!(self.kind, BlockKind::Code { .. })
    }
}

/// A rich text document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    pub blocks: Vec<Block>,
}

impl Document {
    pub fn new() -> Document {
        Document::default()
    }

    /// One-shot Markdown parse (CommonMark + GFM tables, task lists,
    /// strikethrough, autolinks).
    pub fn from_markdown(md: &str) -> Document {
        Document {
            blocks: crate::text::markdown::parse(md),
        }
    }

    pub fn push(&mut self, b: Block) {
        self.blocks.push(b);
    }

    /// Plain text of every block, one block per line ([`Document::ranges`]
    /// gives where each block is in it).
    pub fn plain_text(&self) -> String {
        plain_text(&self.blocks)
    }

    /// Byte range of each block inside [`Document::plain_text`].
    pub fn ranges(&self) -> Vec<Range<usize>> {
        let mut out = Vec::with_capacity(self.blocks.len());
        let mut at = 0;
        for b in &self.blocks {
            out.push(at..at + b.text().len());
            at += b.text().len() + 1;
        }
        out
    }
}

pub(crate) fn plain_text(blocks: &[Block]) -> String {
    let n: usize = blocks.iter().map(|b| b.text().len() + 1).sum();
    let mut s = String::with_capacity(n);
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 {
            s.push('\n');
        }
        s.push_str(b.text());
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_merge_and_cover_the_text() {
        let mut i = Inline::new();
        i.push("a", &Style::plain());
        i.push("b", &Style::plain());
        i.push("c", &Style::plain().bold());
        i.push("x\0\r\ny", &Style::plain().bold());
        assert_eq!(i.text(), "abcx\u{fffd}\ny");
        assert_eq!(i.runs().len(), 2);
        assert_eq!(i.runs()[0].range, 0..2);
        assert_eq!(i.runs()[1].range, 2..i.len());
        assert_eq!(i.run_at(0), Some(0));
        assert_eq!(i.run_at(2), Some(1));
        assert_eq!(i.run_at(99), None);
        i.truncate(1);
        assert_eq!(
            i.runs(),
            &[Run {
                range: 0..1,
                style: Style::plain()
            }]
        );
    }

    #[test]
    fn plain_text_and_ranges_line_up() {
        let mut d = Document::new();
        d.push(Block::heading(1, "Title"));
        d.push(Block::table(
            &[ColumnAlign::Left, ColumnAlign::Right],
            &[&["a", "b"], &["1", "2"]],
            true,
        ));
        let text = d.plain_text();
        assert_eq!(text, "Title\na\tb\n1\t2");
        let r = d.ranges();
        assert_eq!(&text[r[1].clone()], "a\tb\n1\t2");
        let BlockKind::Table(t) = &d.blocks[1].kind else {
            panic!()
        };
        assert_eq!(&d.blocks[1].text()[t.rows[1][1].clone()], "2");
    }
}
