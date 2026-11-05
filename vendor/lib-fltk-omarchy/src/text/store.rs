//! Compact storage of the blocks of a [`RichText`](super::RichText).
//!
//! A long transcript keeps every block, but only the blocks near the view
//! are laid out. The others live here, packed into four arenas shared by
//! the whole document instead of one [`Block`] (a `String`, a `Vec<Run>`
//! and a few small allocations) each:
//!
//! - `text`: the text of every block, one after the other;
//! - `runs`: 4 bytes per styled run (end offset, bold/italic/strike/code,
//!   color role, link marks), 8 bytes in blocks of 4 MiB or more;
//! - `aux`: the rare per-block data (link URLs, ordered list numbers,
//!   code languages, table cells), as bytes;
//! - `metas`: 16 bytes per block (offsets into the arenas, kind, nesting,
//!   marker, flags).
//!
//! Blocks are only ever replaced at the end (a stream re-parses its open
//! tail), so every arena is truncated and appended, never edited in the
//! middle. Blocks are materialized back into a [`Block`] only to be laid
//! out (and those copies are dropped with their layout).
//!
//! Limits: 4 GiB of text per document; heading levels above 7 are stored
//! as 7.

use super::doc::*;
use std::ops::Range;
use std::sync::Arc;

// Run word: style bits, link marks, then the end offset (narrow blocks).
const BOLD: u32 = 1;
const ITALIC: u32 = 2;
const STRIKE: u32 = 4;
const CODE: u32 = 8;
const COLOR_SHIFT: u32 = 4;
/// The run is (part of) a link.
const LINK: u32 = 1 << 8;
/// The run starts a new link (the next URL of the block's link table).
const LINK_START: u32 = 1 << 9;
const STYLE_MASK: u32 = (1 << 10) - 1;
const END_SHIFT: u32 = 10;
/// Blocks at least this long store two words per run.
const NARROW_MAX: usize = 1 << (32 - END_SHIFT);

/// A decoded run: end byte (from the block start) and style bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CRun {
    pub end: usize,
    style: u32,
}

impl CRun {
    fn link(self) -> bool {
        self.style & LINK != 0
    }
    fn link_start(self) -> bool {
        self.style & LINK_START != 0
    }
}

fn color_code(c: Option<ColorRole>) -> u32 {
    // `ColorRole::ALL` is in declaration order.
    c.map_or(0, |c| c as u32 + 1)
}

fn color_of(code: u32) -> Option<ColorRole> {
    (code as usize)
        .checked_sub(1)
        .and_then(|i| ColorRole::ALL.get(i).copied())
}

fn flag_bits(s: &Style) -> u32 {
    (s.bold as u32 * BOLD)
        | (s.italic as u32 * ITALIC)
        | (s.strike as u32 * STRIKE)
        | (s.code as u32 * CODE)
        | (color_code(s.color) << COLOR_SHIFT)
}

/// Style words of `runs` (link marks included) and the URLs they start.
fn encode_runs(runs: &[Run]) -> (Vec<u32>, Vec<&str>) {
    let mut urls: Vec<&str> = Vec::new();
    let mut prev: Option<&str> = None;
    let words = runs
        .iter()
        .map(|r| {
            let mut w = flag_bits(&r.style);
            let link = r.style.link.as_deref();
            if let Some(u) = link {
                w |= LINK;
                if prev != Some(u) {
                    w |= LINK_START;
                    urls.push(u);
                }
            }
            prev = link;
            w
        })
        .collect();
    (words, urls)
}

/// What layout needs to know about a block besides its text and runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShapeKind {
    Paragraph,
    Heading(u8),
    Code,
    Rule,
    /// Number of rows.
    Table(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Shape {
    pub kind: ShapeKind,
    pub indent: u8,
    pub quote: u8,
    pub marker: bool,
}

impl Shape {
    pub fn of(b: &Block) -> Shape {
        Shape {
            kind: match &b.kind {
                BlockKind::Paragraph => ShapeKind::Paragraph,
                BlockKind::Heading(l) => ShapeKind::Heading(*l),
                BlockKind::Code { .. } => ShapeKind::Code,
                BlockKind::Rule => ShapeKind::Rule,
                BlockKind::Table(t) => ShapeKind::Table(t.rows.len().min(u32::MAX as usize) as u32),
            },
            indent: b.indent,
            quote: b.quote,
            marker: b.marker.is_some(),
        }
    }

    pub fn is_code(self) -> bool {
        self.kind == ShapeKind::Code
    }
}

const K_PARAGRAPH: u32 = 0;
const K_HEADING: u32 = 1;
const K_CODE: u32 = 2;
const K_RULE: u32 = 3;
const K_TABLE: u32 = 4;

const M_NONE: u32 = 0;
const M_BULLET: u32 = 1;
const M_ORDERED: u32 = 2;
const M_TASK: u32 = 3;
const M_DONE: u32 = 4;

// Meta bits: kind 0..3, level 3..6, indent 6..14, quote 14..22, tone
// 22..26, marker 26..29, then flags.
const F_LINKS: u32 = 1 << 29;
const F_WIDE: u32 = 1 << 30;
const F_MEASURED: u32 = 1 << 31;
const HEADER_MASK: u32 = (1 << 29) - 1;

/// One block: where its data starts in the arenas (it ends where the next
/// block's starts), and its fixed-size properties packed in `bits`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Meta {
    text: u32,
    runs: u32,
    aux: u32,
    bits: u32,
}

impl Meta {
    fn kind(self) -> u32 {
        self.bits & 7
    }
    fn level(self) -> u8 {
        ((self.bits >> 3) & 7) as u8
    }
    fn indent(self) -> u8 {
        (self.bits >> 6) as u8
    }
    fn quote(self) -> u8 {
        (self.bits >> 14) as u8
    }
    fn tone(self) -> u32 {
        (self.bits >> 22) & 15
    }
    fn marker(self) -> u32 {
        (self.bits >> 26) & 7
    }
}

/// The fixed-size part of a block, as meta bits.
fn header(b: &Block) -> u32 {
    let (kind, level) = match &b.kind {
        BlockKind::Paragraph => (K_PARAGRAPH, 0),
        BlockKind::Heading(l) => (K_HEADING, (*l).min(7) as u32),
        BlockKind::Code { .. } => (K_CODE, 0),
        BlockKind::Rule => (K_RULE, 0),
        BlockKind::Table(_) => (K_TABLE, 0),
    };
    let marker = match b.marker {
        None => M_NONE,
        Some(ListMarker::Bullet) => M_BULLET,
        Some(ListMarker::Ordered(_)) => M_ORDERED,
        Some(ListMarker::Task(false)) => M_TASK,
        Some(ListMarker::Task(true)) => M_DONE,
    };
    kind | level << 3
        | (b.indent as u32) << 6
        | (b.quote as u32) << 14
        | color_code(b.quote_tone) << 22
        | marker << 26
}

fn off(n: usize) -> u32 {
    u32::try_from(n).expect("RichText holds at most 4 GiB of text")
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn get_u32(b: &[u8], at: usize) -> u32 {
    b.get(at..at + 4)
        .map_or(0, |s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// The aux bytes of one block, split.
struct Aux<'a> {
    /// End offsets (u32 each) into `urls`.
    link_ends: &'a [u8],
    urls: &'a [u8],
    number: Option<u64>,
    lang: &'a str,
    table: &'a [u8],
}

impl<'a> Aux<'a> {
    fn links(&self) -> usize {
        self.link_ends.len() / 4
    }

    /// URL of link `k` (0-based).
    fn url(&self, k: usize) -> Option<&'a str> {
        if k >= self.links() {
            return None;
        }
        let s = if k == 0 {
            0
        } else {
            get_u32(self.link_ends, (k - 1) * 4) as usize
        };
        let e = get_u32(self.link_ends, k * 4) as usize;
        std::str::from_utf8(self.urls.get(s..e)?).ok()
    }

    fn table(&self) -> Table {
        let t = self.table;
        let mut at = 0;
        let mut next = || {
            let v = get_u32(t, at);
            at += 4;
            v as usize
        };
        let header_rows = next();
        let ncols = next();
        let mut align = Vec::with_capacity(ncols);
        for _ in 0..ncols {
            align.push(match next() {
                1 => ColumnAlign::Center,
                2 => ColumnAlign::Right,
                _ => ColumnAlign::Left,
            });
        }
        let nrows = next();
        let mut rows = Vec::with_capacity(nrows);
        for _ in 0..nrows {
            let n = next();
            let mut cells = Vec::with_capacity(n);
            for _ in 0..n {
                let s = next();
                let e = next();
                cells.push(s..e);
            }
            rows.push(cells);
        }
        Table {
            align,
            header_rows,
            rows,
        }
    }
}

/// The blocks of a document, packed (see the module docs).
#[derive(Debug, Default)]
pub(crate) struct Store {
    text: String,
    runs: Vec<u32>,
    aux: Vec<u8>,
    metas: Vec<Meta>,
}

impl Store {
    pub fn len(&self) -> usize {
        self.metas.len()
    }

    pub fn clear(&mut self) {
        // Give the memory back: a cleared view may stay small.
        *self = Store::default();
    }

    fn ends(&self, i: usize) -> (usize, usize, usize) {
        match self.metas.get(i + 1) {
            Some(n) => (n.text as usize, n.runs as usize, n.aux as usize),
            None => (self.text.len(), self.runs.len(), self.aux.len()),
        }
    }

    pub fn text(&self, i: usize) -> &str {
        let m = &self.metas[i];
        &self.text[m.text as usize..self.ends(i).0]
    }

    /// The runs of block `i`, in order.
    pub fn runs(&self, i: usize) -> impl Iterator<Item = CRun> + '_ {
        let m = self.metas[i];
        let words = &self.runs[m.runs as usize..self.ends(i).1];
        let wide = m.bits & F_WIDE != 0;
        let step = if wide { 2 } else { 1 };
        words.chunks_exact(step).map(move |c| {
            if wide {
                CRun {
                    end: c[1] as usize,
                    style: c[0] & STYLE_MASK,
                }
            } else {
                CRun {
                    end: (c[0] >> END_SHIFT) as usize,
                    style: c[0] & STYLE_MASK,
                }
            }
        })
    }

    /// Whether block `i` has a measured height (in the view's sums).
    pub fn measured(&self, i: usize) -> bool {
        self.metas[i].bits & F_MEASURED != 0
    }

    pub fn set_measured(&mut self, i: usize, on: bool) {
        let m = &mut self.metas[i];
        if on {
            m.bits |= F_MEASURED;
        } else {
            m.bits &= !F_MEASURED;
        }
    }

    fn aux(&self, i: usize) -> Aux<'_> {
        let m = self.metas[i];
        let b = &self.aux[m.aux as usize..self.ends(i).2];
        let mut at = 0;
        let (mut link_ends, mut urls): (&[u8], &[u8]) = (&[], &[]);
        if m.bits & F_LINKS != 0 {
            let n = get_u32(b, 0) as usize;
            link_ends = &b[4..4 + 4 * n];
            let len = if n == 0 {
                0
            } else {
                get_u32(link_ends, (n - 1) * 4) as usize
            };
            urls = &b[4 + 4 * n..4 + 4 * n + len];
            at = 4 + 4 * n + len;
        }
        let mut number = None;
        if m.marker() == M_ORDERED {
            let mut x = [0u8; 8];
            x.copy_from_slice(&b[at..at + 8]);
            number = Some(u64::from_le_bytes(x));
            at += 8;
        }
        let mut lang = "";
        if m.kind() == K_CODE {
            let n = get_u32(b, at) as usize;
            lang = std::str::from_utf8(&b[at + 4..at + 4 + n]).unwrap_or("");
            at += 4 + n;
        }
        let table = if m.kind() == K_TABLE {
            &b[at..]
        } else {
            &[][..]
        };
        Aux {
            link_ends,
            urls,
            number,
            lang,
            table,
        }
    }

    pub fn shape(&self, i: usize) -> Shape {
        let m = self.metas[i];
        Shape {
            kind: match m.kind() {
                K_HEADING => ShapeKind::Heading(m.level()),
                K_CODE => ShapeKind::Code,
                K_RULE => ShapeKind::Rule,
                K_TABLE => {
                    let t = self.aux(i).table;
                    let ncols = get_u32(t, 4) as usize;
                    ShapeKind::Table(get_u32(t, 8 + 4 * ncols))
                }
                _ => ShapeKind::Paragraph,
            },
            indent: m.indent(),
            quote: m.quote(),
            marker: m.marker() != M_NONE,
        }
    }

    pub fn marker(&self, i: usize) -> Option<ListMarker> {
        match self.metas[i].marker() {
            M_BULLET => Some(ListMarker::Bullet),
            M_ORDERED => Some(ListMarker::Ordered(self.aux(i).number.unwrap_or(1))),
            M_TASK => Some(ListMarker::Task(false)),
            M_DONE => Some(ListMarker::Task(true)),
            _ => None,
        }
    }

    /// URL of the link of run `run` of block `i`.
    pub fn link(&self, i: usize, run: usize) -> Option<&str> {
        let mut k = 0;
        for (ri, c) in self.runs(i).enumerate() {
            k += c.link_start() as usize;
            if ri == run {
                return if c.link() {
                    self.aux(i).url(k.checked_sub(1)?)
                } else {
                    None
                };
            }
        }
        None
    }

    /// Appends a block.
    pub fn push(&mut self, b: &Block) {
        let text = b.text();
        let wide = text.len() >= NARROW_MAX;
        let mut meta = Meta {
            text: off(self.text.len()),
            runs: off(self.runs.len()),
            aux: off(self.aux.len()),
            bits: header(b) | if wide { F_WIDE } else { 0 },
        };
        self.text.push_str(text);
        // Same words as `encode_runs`, without its two vectors.
        let mut urls: Vec<&str> = Vec::new();
        let mut prev: Option<&str> = None;
        let runs = b.content.runs();
        self.runs.reserve(runs.len() * if wide { 2 } else { 1 });
        for r in runs {
            let mut w = flag_bits(&r.style);
            let link = r.style.link.as_deref();
            if let Some(u) = link {
                w |= LINK;
                if prev != Some(u) {
                    w |= LINK_START;
                    urls.push(u);
                }
            }
            prev = link;
            if wide {
                self.runs.push(w);
                self.runs.push(off(r.range.end));
            } else {
                self.runs.push(w | (r.range.end as u32) << END_SHIFT);
            }
        }
        if !urls.is_empty() {
            meta.bits |= F_LINKS;
            put_u32(&mut self.aux, off(urls.len()));
            let mut end = 0u32;
            for u in &urls {
                end = end.saturating_add(off(u.len()));
                put_u32(&mut self.aux, end);
            }
            for u in &urls {
                self.aux.extend_from_slice(u.as_bytes());
            }
        }
        if let Some(ListMarker::Ordered(n)) = b.marker {
            self.aux.extend_from_slice(&n.to_le_bytes());
        }
        match &b.kind {
            BlockKind::Code { lang } => {
                put_u32(&mut self.aux, off(lang.len()));
                self.aux.extend_from_slice(lang.as_bytes());
            }
            BlockKind::Table(t) => {
                put_u32(&mut self.aux, off(t.header_rows));
                put_u32(&mut self.aux, off(t.align.len()));
                for a in &t.align {
                    put_u32(
                        &mut self.aux,
                        match a {
                            ColumnAlign::Left => 0,
                            ColumnAlign::Center => 1,
                            ColumnAlign::Right => 2,
                        },
                    );
                }
                put_u32(&mut self.aux, off(t.rows.len()));
                for row in &t.rows {
                    put_u32(&mut self.aux, off(row.len()));
                    for c in row {
                        put_u32(&mut self.aux, off(c.start));
                        put_u32(&mut self.aux, off(c.end));
                    }
                }
            }
            _ => {}
        }
        self.metas.push(meta);
    }

    /// Keeps the first `n` blocks.
    pub fn truncate(&mut self, n: usize) {
        let Some(m) = self.metas.get(n).copied() else {
            return;
        };
        self.text.truncate(m.text as usize);
        self.runs.truncate(m.runs as usize);
        self.aux.truncate(m.aux as usize);
        self.metas.truncate(n);
    }

    fn kind(&self, i: usize, aux: &Aux) -> BlockKind {
        let m = self.metas[i];
        match m.kind() {
            K_HEADING => BlockKind::Heading(m.level()),
            K_CODE => BlockKind::Code {
                lang: aux.lang.to_string(),
            },
            K_RULE => BlockKind::Rule,
            K_TABLE => BlockKind::Table(Box::new(aux.table())),
            _ => BlockKind::Paragraph,
        }
    }

    /// Block `i` as a [`Block`] (to lay it out or hand it out).
    pub fn block(&self, i: usize) -> Block {
        let m = self.metas[i];
        let aux = self.aux(i);
        let mut start = 0usize;
        let mut k = 0usize;
        let mut link: Option<Arc<str>> = None;
        let runs = self
            .runs(i)
            .map(|c| {
                if c.link_start() {
                    link = aux.url(k).map(Arc::from);
                    k += 1;
                }
                let r = Run {
                    range: start..c.end,
                    style: Style {
                        bold: c.style & BOLD != 0,
                        italic: c.style & ITALIC != 0,
                        strike: c.style & STRIKE != 0,
                        code: c.style & CODE != 0,
                        link: if c.link() { link.clone() } else { None },
                        color: color_of((c.style >> COLOR_SHIFT) & 15),
                    },
                };
                start = c.end;
                r
            })
            .collect();
        Block {
            kind: self.kind(i, &aux),
            content: Inline::from_parts(self.text(i).to_string(), runs),
            indent: m.indent(),
            quote: m.quote(),
            quote_tone: color_of(m.tone()),
            marker: self.marker(i),
        }
    }

    /// True when block `i` equals `b` (without building it).
    pub fn same(&self, i: usize, b: &Block) -> bool {
        let m = self.metas[i];
        if header(b) != m.bits & HEADER_MASK || self.text(i) != b.text() {
            return false;
        }
        let runs = b.content.runs();
        if self.runs(i).count() != runs.len() {
            return false;
        }
        let aux = self.aux(i);
        if let Some(ListMarker::Ordered(n)) = b.marker {
            if aux.number != Some(n) {
                return false;
            }
        }
        match &b.kind {
            BlockKind::Code { lang } if aux.lang != lang => return false,
            BlockKind::Table(t) if **t != aux.table() => return false,
            _ => {}
        }
        let (words, urls) = encode_runs(runs);
        urls.len() == aux.links()
            && urls.iter().enumerate().all(|(k, u)| aux.url(k) == Some(u))
            && self
                .runs(i)
                .zip(words.iter().zip(runs))
                .all(|(c, (w, r))| c.end == r.range.end && c.style == *w)
    }

    /// Cells of table block `i` (rows of byte ranges) and its header rows.
    #[cfg_attr(not(feature = "a11y"), allow(dead_code))]
    pub fn table(&self, i: usize) -> Option<Table> {
        (self.metas[i].kind() == K_TABLE).then(|| self.aux(i).table())
    }

    /// The links of block `i`: byte range, run range, URL.
    #[cfg_attr(not(feature = "a11y"), allow(dead_code))]
    pub fn links(&self, i: usize) -> Vec<(Range<usize>, Range<usize>, &str)> {
        let aux = self.aux(i);
        let mut out: Vec<(Range<usize>, Range<usize>, &str)> = Vec::new();
        let mut start = 0usize;
        for (ri, c) in self.runs(i).enumerate() {
            if c.link_start() {
                out.push((start..c.end, ri..ri + 1, aux.url(out.len()).unwrap_or("")));
            } else if c.link() {
                if let Some(l) = out.last_mut() {
                    l.0.end = c.end;
                    l.1.end = ri + 1;
                }
            }
            start = c.end;
        }
        out
    }

    /// Code block language.
    #[cfg_attr(not(feature = "a11y"), allow(dead_code))]
    pub fn lang(&self, i: usize) -> &str {
        self.aux(i).lang
    }

    /// Bytes held (lengths, not capacities: what a long document touches).
    #[cfg(test)]
    pub fn bytes(&self) -> usize {
        self.text.len() + self.runs.len() * 4 + self.aux.len() + self.metas.len() * 16
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::markdown::parse;

    const MD: &str = "# Title\n\nSome **bold** and *it* with [a link](https://a.dev) and https://b.dev.\n\n1. one\n2. two\n   - [x] done\n\n```rust\nfn main() {}\n```\n\n| a | b |\n|:-|-:|\n| 1 | [x](https://c.dev) |\n\n> [!NOTE]\n> careful\n\n---\n";

    #[test]
    fn blocks_round_trip_and_compare() {
        let blocks = parse(MD);
        let mut s = Store::default();
        for b in &blocks {
            s.push(b);
        }
        assert_eq!(s.len(), blocks.len());
        for (i, b) in blocks.iter().enumerate() {
            assert_eq!(&s.block(i), b, "block {i}");
            assert!(s.same(i, b), "same {i}");
            assert_eq!(s.text(i), b.text());
            assert_eq!(s.shape(i), Shape::of(b));
            assert_eq!(s.marker(i), b.marker);
            for (ri, r) in b.content.runs().iter().enumerate() {
                assert_eq!(s.link(i, ri), r.style.link.as_deref());
            }
        }
        let p = blocks
            .iter()
            .position(|b| b.text().contains("bold"))
            .unwrap();
        let mut other = blocks[p].clone();
        other.content = Inline::plain(blocks[p].text());
        assert!(!s.same(p, &other));
        let links: Vec<&str> = s.links(p).iter().map(|l| l.2).collect();
        assert_eq!(links, ["https://a.dev", "https://b.dev"]);
        let (range, runs, _) = &s.links(p)[0];
        assert_eq!(&blocks[p].text()[range.clone()], "a link");
        assert_eq!(s.link(p, runs.start), Some("https://a.dev"));
        // Truncate and push again: the same bytes.
        let before = s.bytes();
        s.truncate(2);
        assert_eq!(s.len(), 2);
        for b in &blocks[2..] {
            s.push(b);
        }
        assert_eq!(s.bytes(), before);
        assert_eq!(s.block(blocks.len() - 1), blocks[blocks.len() - 1]);
    }

    #[test]
    fn huge_blocks_use_wide_runs() {
        let big = "x".repeat(NARROW_MAX + 10);
        let b = Block::paragraph(&big)
            .push("bold", &Style::plain().bold())
            .push("link", &Style::plain().link("https://a.dev"));
        let mut s = Store::default();
        s.push(&Block::heading(2, "before"));
        s.push(&b);
        s.push(&Block::paragraph("after"));
        assert_eq!(s.block(1), b);
        assert!(s.same(1, &b));
        assert_eq!(s.link(1, 2), Some("https://a.dev"));
        assert_eq!(s.text(2), "after");
    }

    #[test]
    fn dense_markdown_costs_under_two_bytes_per_byte() {
        // The stream of examples/richtext_bench.rs.
        let unit = "Streaming a long answer with **bold**, *italic*, `inline code` and a link to https://example.com/page in the middle of a sentence that keeps going for a while. More words arrive token by token, each one appended to the last paragraph of the message. \n\n- a list item with `code`\n- another item\n- third item with [a link](https://x.dev)\n\n```python\nfor i in range(10):\n    print(i)  # comment\n```\n\nAnother paragraph of plain text to fill the transcript with something realistic to lay out and draw. \n\n";
        let md = unit.repeat(200);
        let mut s = Store::default();
        for b in parse(&md) {
            s.push(&b);
        }
        // + 8 bytes per block for the view's height sums.
        let per_byte = (s.bytes() + 8 * s.len()) as f64 / md.len() as f64;
        assert!(
            per_byte < 1.6,
            "{per_byte:.2} B/B: md {} text {} runs {} aux {} blocks {}",
            md.len(),
            s.text.len(),
            s.runs.len(),
            s.aux.len(),
            s.len()
        );
        assert_eq!(std::mem::size_of::<Meta>(), 16);
        for (i, c) in ColorRole::ALL.iter().enumerate() {
            assert_eq!(color_of(color_code(Some(*c))), Some(*c), "{i}");
        }
    }
}
