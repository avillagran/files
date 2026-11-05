//! Line layout of one block: word wrap with FLTK text measurement, CJK
//! breaks between ideographs, grapheme-safe breaks inside words that do not
//! fit, tab stops in code. Long code lines wrap like prose (no horizontal
//! scrolling).
//!
//! A layout is relative to the block's box: x = 0 is the left edge of the
//! view's text column (indentation is inside the layout), y = 0 its top.
//! When a block is replaced by a longer version of itself (streaming), the
//! lines before the change are reused and only the rest is laid out again.
//!
//! Each fragment of a line is drawn as one string (one run, one line), so
//! FLTK (Pango on Linux) shapes it: Arabic joins.
//!
//! Bidirectional text follows the Unicode Bidirectional Algorithm (UAX #9,
//! `unicode-bidi`): a block's base direction is its first strong character
//! (`dir=auto`; start a block with U+200F RLM or U+200E LRM to force it).
//! Blocks with right-to-left text get their embedding levels per paragraph;
//! each line is split where the level changes and its fragments are placed
//! in visual order (rule L2), so Arabic, Latin and numbers on one line read
//! right. Every fragment then has one direction (`Frag::rtl`). Lines of a
//! right-to-left block are right-aligned and their list/quote indentation
//! is on the right ([`Layout::mirror_x`]); code blocks stay left to right.
//! Hit testing, caret x and selection rectangles follow the visual order.
//! Text without right-to-left characters skips all of it.

use super::doc::*;
use super::store::{Shape, ShapeKind};
use crate::theme::Theme;
use fltk::enums::Font;
use std::ops::Range;
use unicode_bidi::{BidiInfo, Level};
use unicode_segmentation::UnicodeSegmentation;

/// A font: family (UI or monospace), weight, slant, size in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FontKey {
    pub mono: bool,
    pub bold: bool,
    pub italic: bool,
    pub size: i32,
}

impl FontKey {
    pub fn fltk(self) -> Font {
        match (self.mono, self.bold, self.italic) {
            (false, false, false) => Font::Helvetica,
            (false, true, false) => Font::HelveticaBold,
            (false, false, true) => Font::HelveticaItalic,
            (false, true, true) => Font::HelveticaBoldItalic,
            (true, false, false) => Font::Courier,
            (true, true, false) => Font::CourierBold,
            (true, false, true) => Font::CourierItalic,
            (true, true, true) => Font::CourierBoldItalic,
        }
    }

    pub fn with(self, s: &Style) -> FontKey {
        FontKey {
            mono: self.mono || s.code,
            bold: self.bold || s.bold,
            italic: self.italic || s.italic,
            size: self.size,
        }
    }
}

/// Text measurement (FLTK in the app, a fixed-pitch fake in unit tests).
pub trait Metrics {
    fn width(&mut self, f: FontKey, s: &str) -> f32;
    /// (ascent, descent) in pixels.
    fn vmetrics(&mut self, f: FontKey) -> (i32, i32);
}

extern "C" {
    // FLTK's fl_width(const char*, int) through cfltk: no allocation.
    fn Fl_width2(txt: *const std::ffi::c_char, n: std::ffi::c_int) -> f64;
}

/// FLTK measurement with the current graphics driver (needs an open
/// display; fine inside draw and event callbacks).
#[derive(Default)]
pub struct FltkMetrics {
    v: Vec<(FontKey, (i32, i32))>,
}

impl FltkMetrics {
    pub fn set(f: FontKey) {
        let font = f.fltk();
        if fltk::draw::font() != font || fltk::draw::size() != f.size {
            fltk::draw::set_font(font, f.size);
        }
    }
}

impl Metrics for FltkMetrics {
    fn width(&mut self, f: FontKey, s: &str) -> f32 {
        if s.is_empty() {
            return 0.0;
        }
        Self::set(f);
        // SAFETY: pointer and length of a live &str; FLTK reads n bytes.
        unsafe {
            Fl_width2(
                s.as_ptr() as *const std::ffi::c_char,
                s.len() as std::ffi::c_int,
            ) as f32
        }
    }

    fn vmetrics(&mut self, f: FontKey) -> (i32, i32) {
        if let Some((_, m)) = self.v.iter().find(|(k, _)| *k == f) {
            return *m;
        }
        Self::set(f);
        let (h, d) = (fltk::draw::height(), fltk::draw::descent());
        let m = ((h - d).max(1), d.max(0));
        self.v.push((f, m));
        m
    }
}

/// Fixed pitch: every char is `size * 0.6` wide (CJK double), for tests.
#[derive(Default)]
pub struct FakeMetrics;

impl Metrics for FakeMetrics {
    fn width(&mut self, f: FontKey, s: &str) -> f32 {
        s.chars()
            .map(|c| if is_cjk(c) { 2.0 } else { 1.0 })
            .sum::<f32>()
            * f.size as f32
            * 0.6
            + if f.bold {
                s.chars().count() as f32 * 0.5
            } else {
                0.0
            }
    }
    fn vmetrics(&mut self, f: FontKey) -> (i32, i32) {
        (f.size * 4 / 5 + 1, f.size / 4)
    }
}

/// Sizes and spacing taken from the theme.
#[derive(Debug, Clone, PartialEq)]
pub struct Typo {
    pub body: i32,
    pub small: i32,
    pub heading: [i32; 6],
    pub line: f32,
    pub gap: i32,
    pub item_gap: i32,
    pub list_w: i32,
    pub quote_w: i32,
    pub code_pad: i32,
    pub code_header: i32,
    pub cell_pad: i32,
    pub family: String,
}

impl Typo {
    pub fn from_theme(t: &Theme) -> Typo {
        let f = &t.font;
        let s = &t.spacing;
        Typo {
            body: f.body,
            small: f.caption,
            heading: [f.heading, f.title, f.subtitle, f.body, f.body, f.body_small],
            line: 1.45,
            gap: s.xl,
            item_gap: s.xs,
            list_w: (f.body as f32 * 2.2).round() as i32,
            quote_w: s.xl + 4,
            code_pad: s.lg,
            code_header: f.caption + 2 * s.sm + 2,
            cell_pad: s.md,
            family: t.font_family.clone(),
        }
    }

    /// Base font of a block's text.
    pub fn base(&self, b: &Block) -> FontKey {
        self.base_of(Shape::of(b))
    }

    pub(crate) fn base_of(&self, s: Shape) -> FontKey {
        match s.kind {
            ShapeKind::Heading(l) => FontKey {
                bold: true,
                size: self.heading[(l as usize).clamp(1, 6) - 1],
                ..Default::default()
            },
            ShapeKind::Code => FontKey {
                mono: true,
                size: self.body,
                ..Default::default()
            },
            _ => FontKey {
                size: self.body,
                ..Default::default()
            },
        }
    }

    /// Left edge of the text of `b` (quotes and list nesting).
    pub fn left(&self, b: &Block) -> i32 {
        self.left_of(Shape::of(b))
    }

    pub(crate) fn left_of(&self, s: Shape) -> i32 {
        s.quote as i32 * self.quote_w + s.indent as i32 * self.list_w
    }

    /// Space above `b` when it follows `prev`.
    pub fn gap(&self, prev: Option<&Block>, b: &Block) -> i32 {
        self.gap_of(prev.map(Shape::of), Shape::of(b))
    }

    pub(crate) fn gap_of(&self, prev: Option<Shape>, b: Shape) -> i32 {
        match prev {
            None => 0,
            Some(p)
                if p.indent > 0 && b.indent > 0 && !p.is_code() && !b.is_code() && b.marker =>
            {
                self.item_gap
            }
            Some(_) if matches!(b.kind, ShapeKind::Heading(l) if l <= 2) => self.gap * 2,
            Some(_) => self.gap,
        }
    }

    pub fn line_h(&self, size: i32) -> i32 {
        (size as f32 * self.line).round() as i32
    }
}

/// A piece of a line in one font and one run, drawn as one string.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Frag {
    /// From the line's x.
    pub x: f32,
    pub w: f32,
    /// Drawn bytes (trailing blanks at a line end are not drawn).
    pub start: u32,
    pub end: u32,
    pub run: u32,
    pub font: FontKey,
    pub tab: bool,
    /// Right-to-left run (odd bidi level): byte `start` is at the right edge.
    pub rtl: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Line {
    pub x: i32,
    pub y: i32,
    /// Width available to the line (the column or the table cell).
    pub w: i32,
    pub h: i32,
    /// Baseline from the line's top.
    pub base: i32,
    /// Bytes of the text covered, including trailing blanks and the newline.
    pub start: u32,
    pub end: u32,
    /// Fragments in visual order (left to right on screen).
    pub frags: Range<u32>,
    /// Right-to-left paragraph: right-aligned, its logical end on the left.
    pub rtl: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TableGeom {
    /// (x, w) of each column.
    pub cols: Vec<(i32, i32)>,
    /// (y, h) of each row.
    pub rows: Vec<(i32, i32)>,
    pub header_rows: usize,
}

/// The laid out block.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    /// Width it was laid out for.
    pub width: i32,
    pub height: i32,
    /// Text box (left edge and width; code blocks: the surface).
    pub left: i32,
    pub box_w: i32,
    pub lines: Vec<Line>,
    pub frags: Vec<Frag>,
    pub table: Option<TableGeom>,
    /// Right-to-left block (first strong character): lines right-aligned,
    /// indentation on the right.
    pub rtl: bool,
}

impl Layout {
    pub fn frags_of(&self, l: &Line) -> &[Frag] {
        &self.frags[l.frags.start as usize..l.frags.end as usize]
    }

    /// Mirrors the span `x..x + w` (block coordinates) inside the block for
    /// a right-to-left block: list markers and quote bars go on the right.
    pub fn mirror_x(&self, x: i32, w: i32) -> i32 {
        if self.rtl {
            self.width - x - w
        } else {
            x
        }
    }

    /// Byte offset nearest to (x, y) (block coordinates).
    pub fn hit(&self, text: &str, x: i32, y: i32, m: &mut dyn Metrics) -> usize {
        let Some(li) = self.line_at(x, y) else {
            return 0;
        };
        let l = &self.lines[li];
        let frags = self.frags_of(l);
        let rx = (x - l.x) as f32;
        let line_end = if text[..l.end as usize].ends_with('\n') {
            l.end as usize - 1
        } else {
            l.end as usize
        }
        .max(l.start as usize);
        let (Some(first), Some(last)) = (frags.first(), frags.last()) else {
            return l.start as usize;
        };
        // Beyond either end: the logical start or end of the line.
        let (left_end, right_end) = if l.rtl {
            (line_end, l.start as usize)
        } else {
            (l.start as usize, line_end)
        };
        if rx <= first.x {
            return left_end;
        }
        if rx >= last.x + last.w {
            return right_end;
        }
        for f in frags {
            if rx < f.x + f.w {
                if f.tab {
                    return if rx < f.x + f.w / 2.0 {
                        f.start as usize
                    } else {
                        f.end as usize
                    };
                }
                // Distance from the fragment's logical start edge.
                let d = if f.rtl { f.x + f.w - rx } else { rx - f.x };
                let s = &text[f.start as usize..f.end as usize];
                let mut best = f.start as usize;
                let mut prev_w = 0.0;
                for (i, g) in s.grapheme_indices(true) {
                    let end = i + g.len();
                    let w = m.width(f.font, &s[..end]);
                    // Nearest edge of the grapheme under the pointer.
                    if d < w {
                        let mid = (prev_w + w) / 2.0;
                        return f.start as usize + if d < mid { i } else { end };
                    }
                    best = f.start as usize + end;
                    prev_w = w;
                }
                return best;
            }
        }
        right_end
    }

    /// The line under (x, y), or the nearest one.
    pub fn line_at(&self, x: i32, y: i32) -> Option<usize> {
        if self.lines.is_empty() {
            return None;
        }
        if self.table.is_some() {
            // Cells: the line whose cell holds the point, else the nearest by y.
            let mut best = (0usize, i64::MAX);
            for (i, l) in self.lines.iter().enumerate() {
                let dx = if x < l.x {
                    (l.x - x) as i64
                } else if x >= l.x + l.w {
                    (x - l.x - l.w) as i64
                } else {
                    0
                };
                let dy = if y < l.y {
                    (l.y - y) as i64
                } else if y >= l.y + l.h {
                    (y - l.y - l.h) as i64
                } else {
                    0
                };
                let d = dx * 4 + dy * 1000;
                if d < best.1 {
                    best = (i, d);
                }
            }
            return Some(best.0);
        }
        let i = self.lines.partition_point(|l| l.y + l.h <= y);
        Some(i.min(self.lines.len() - 1))
    }

    /// X (from the fragment's x) of byte `at` inside fragment `f`.
    fn in_frag(f: &Frag, text: &str, at: usize, m: &mut dyn Metrics) -> f32 {
        let d = if at <= f.start as usize {
            0.0
        } else if at >= f.end as usize || f.tab {
            f.w
        } else {
            m.width(f.font, &text[f.start as usize..at])
        };
        if f.rtl {
            f.x + f.w - d
        } else {
            f.x + d
        }
    }

    /// X (block coordinates) of byte `at` on line `li` (the caret; in mixed
    /// text, the edge of the character at `at`).
    pub fn x_of(&self, text: &str, li: usize, at: usize, m: &mut dyn Metrics) -> f32 {
        let l = &self.lines[li];
        let frags = self.frags_of(l);
        if let Some(f) = frags
            .iter()
            .find(|f| (f.start as usize) <= at && at < f.end as usize)
            .or_else(|| frags.iter().find(|f| f.end as usize == at))
        {
            return l.x as f32 + Self::in_frag(f, text, at, m);
        }
        // Past the drawn text (trailing blanks): the line's logical end.
        let end = if l.rtl {
            frags.first().map(|f| f.x)
        } else {
            frags.last().map(|f| f.x + f.w)
        };
        l.x as f32 + end.unwrap_or(0.0)
    }

    /// Rectangles (x, y, w, h) covering bytes `range` (selection). In mixed
    /// direction lines a range can cover several separate spans.
    pub fn rects(
        &self,
        text: &str,
        range: Range<usize>,
        m: &mut dyn Metrics,
    ) -> Vec<(i32, i32, i32, i32)> {
        let mut out: Vec<(i32, i32, i32, i32)> = Vec::new();
        for l in self.lines.iter() {
            let (ls, le) = (l.start as usize, l.end as usize);
            if range.end <= ls || range.start >= le.max(ls + 1) {
                continue;
            }
            let frags = self.frags_of(l);
            let mut spans: Vec<(f32, f32)> = Vec::new();
            for f in frags {
                let s = range.start.max(f.start as usize);
                let e = range.end.min(f.end as usize);
                if s < e {
                    let (a, b) = (Self::in_frag(f, text, s, m), Self::in_frag(f, text, e, m));
                    spans.push((a.min(b), a.max(b)));
                }
            }
            // Selected line break or wrap: a bit of the line end shows it.
            if range.end > le || (range.end == le && le > ls && text[..le].ends_with('\n')) {
                let bit = (l.h / 3).max(3) as f32;
                if l.rtl {
                    let x = frags.first().map_or(0.0, |f| f.x);
                    spans.push((x - bit, x));
                } else {
                    let x = frags.last().map_or(0.0, |f| f.x + f.w);
                    spans.push((x, x + bit));
                }
            }
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut merged: Vec<(f32, f32)> = Vec::new();
            for (a, b) in spans {
                match merged.last_mut() {
                    Some(last) if a <= last.1 + 0.5 => last.1 = last.1.max(b),
                    _ => merged.push((a, b)),
                }
            }
            for (a, b) in merged {
                let (x0, x1) = (l.x as f32 + a, l.x as f32 + b);
                if x1 > x0 {
                    out.push((x0.floor() as i32, l.y, (x1.ceil() - x0.floor()) as i32, l.h));
                }
            }
        }
        out
    }

    /// The span (x0, x1), block coordinates, of bytes `range` on line `li`
    /// (all of its pieces in mixed text): tests and "where is this word".
    pub fn span_x(&self, text: &str, li: usize, range: Range<usize>, m: &mut dyn Metrics) -> (f32, f32) {
        let l = &self.lines[li];
        let mut out: Option<(f32, f32)> = None;
        for f in self.frags_of(l) {
            let s = range.start.max(f.start as usize);
            let e = range.end.min(f.end as usize);
            if s < e {
                let (a, b) = (Self::in_frag(f, text, s, m), Self::in_frag(f, text, e, m));
                let (a, b) = (a.min(b), a.max(b));
                out = Some(out.map_or((a, b), |(x0, x1)| (x0.min(a), x1.max(b))));
            }
        }
        let (a, b) = out.unwrap_or_else(|| {
            let x = self.x_of(text, li, range.start, m) - l.x as f32;
            (x, x)
        });
        (l.x as f32 + a, l.x as f32 + b)
    }
}

// ------------------------------------------------------------- the block

/// Lays out `b` for a column `width` pixels wide. `prev` is an earlier
/// layout of an earlier version of the same block (streaming): the lines
/// before the first change are reused.
pub fn layout_block(
    b: &Block,
    width: i32,
    ty: &Typo,
    m: &mut dyn Metrics,
    prev: Option<(&Block, &Layout)>,
) -> Layout {
    let width = width.max(40);
    let left = ty.left(b).min(width - 20);
    let base = ty.base(b);
    let text = b.text();
    let rtl = block_rtl(b);
    let mut out = Layout {
        width,
        left,
        box_w: width - left,
        rtl,
        ..Default::default()
    };
    let reuse = prev.filter(|(pb, pl)| {
        pl.width == width
            && pb.kind == b.kind
            && pb.indent == b.indent
            && pb.quote == b.quote
            && pl.table.is_none()
            && pl.rtl == rtl
    });
    match &b.kind {
        BlockKind::Rule => {
            out.height = ty.gap;
        }
        BlockKind::Table(t) => layout_table(b, t, &mut out, ty, m),
        kind => {
            let code = matches!(kind, BlockKind::Code { .. });
            let (x, w, y0) = if code {
                (
                    left + ty.code_pad,
                    width - left - 2 * ty.code_pad,
                    ty.code_header,
                )
            } else if rtl {
                // Indentation on the right.
                (0, width - left, 0)
            } else {
                (left, width - left, 0)
            };
            let mut from = 0usize;
            let mut y = y0;
            if let Some((pb, pl)) = reuse {
                let common = common_prefix(&pb.content, &b.content);
                // Keep every line but the last one that lies inside the common prefix.
                let full = pl
                    .lines
                    .iter()
                    .take_while(|l| (l.end as usize) <= common)
                    .count();
                if full >= 2 {
                    let keep = full - 1;
                    out.lines.extend_from_slice(&pl.lines[..keep]);
                    let fend = pl.lines[keep - 1].frags.end as usize;
                    out.frags.extend_from_slice(&pl.frags[..fend]);
                    from = pl.lines[keep].start as usize;
                    y = pl.lines[keep].y;
                }
            }
            let bidi = Bidi::new(text, 0, from..text.len(), rtl, code);
            let y = layout_text_in(
                text,
                b.content.runs(),
                from..text.len(),
                base,
                x,
                w,
                y,
                ty,
                m,
                &mut out,
                &bidi,
            );
            out.height = if code { y + ty.code_pad } else { y };
            if let BlockKind::Heading(l) = kind {
                if *l <= 2 {
                    out.height += ty.item_gap + 1 + ty.item_gap;
                }
            }
        }
    }
    out
}

/// Right-to-left block: the first strong character of its text (code
/// blocks are always left to right).
fn block_rtl(b: &Block) -> bool {
    if b.is_code() || !has_rtl(b.text()) {
        return false;
    }
    unicode_bidi::get_base_direction_full(b.text()) == unicode_bidi::Direction::Rtl
}

/// Right-to-left letters (Hebrew, Arabic, Syriac, Thaana, N'Ko… and their
/// presentation forms) or marks that may make text right to left. A quick
/// test: text without any of them skips the bidi algorithm.
pub fn has_rtl(s: &str) -> bool {
    if s.is_ascii() {
        return false;
    }
    s.chars().any(|c| {
        matches!(c as u32,
            0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x10800..=0x10FFF | 0x1E800..=0x1EFFF
            | 0x200F | 0x202B | 0x202E | 0x2067 | 0x2068)
    })
}

/// Bidi levels of the text being laid out (a block or a table cell).
struct Bidi {
    /// Levels were computed (the text has right-to-left characters, or
    /// the block is right to left).
    on: bool,
    /// Base direction: right to left.
    rtl: bool,
    /// Byte offset of `levels[0]` in the text.
    ctx: usize,
    /// One level per byte from `ctx`.
    levels: Vec<Level>,
}

impl Bidi {
    /// Levels for bytes `range` of `text`. The algorithm runs from the start
    /// of the paragraph holding `range.start` (not before `floor`): the
    /// levels of a paragraph depend only on its own text, so an incremental
    /// layout gets the same levels as a full one.
    fn new(text: &str, floor: usize, range: Range<usize>, rtl: bool, code: bool) -> Bidi {
        let off = Bidi { on: false, rtl: false, ctx: range.start, levels: Vec::new() };
        if code {
            return off;
        }
        let ctx = text[floor..range.start]
            .char_indices()
            .rev()
            .find(|(_, c)| matches!(c, '\n' | '\r' | '\u{85}' | '\u{1c}'..='\u{1e}' | '\u{2029}'))
            .map_or(floor, |(i, c)| floor + i + c.len_utf8());
        let part = &text[ctx..range.end];
        if !rtl && !has_rtl(part) {
            return off;
        }
        let level = if rtl { Level::rtl() } else { Level::ltr() };
        let levels = BidiInfo::new(part, Some(level)).levels;
        Bidi { on: true, rtl, ctx, levels }
    }

    fn level(&self, at: usize) -> u8 {
        self.levels.get(at.wrapping_sub(self.ctx)).map_or(if self.rtl { 1 } else { 0 }, |l| l.number())
    }
}

/// Splits the fragments of one line (from `f0`) where the bidi level
/// changes and places them in visual order (rule L2).
fn reorder_line(text: &str, out: &mut Layout, f0: usize, bidi: &Bidi, m: &mut dyn Metrics) {
    let base = if bidi.rtl { 1 } else { 0 };
    let mut pieces: Vec<(Frag, u8)> = Vec::with_capacity(out.frags.len() - f0 + 2);
    for f in out.frags.drain(f0..) {
        if f.tab {
            // L1: segment separators take the paragraph level.
            pieces.push((Frag { rtl: bidi.rtl, ..f }, base));
            continue;
        }
        let (start, end) = (f.start as usize, f.end as usize);
        let mut s = start;
        while s < end {
            let l = bidi.level(s);
            let mut e = s;
            while e < end && bidi.level(e) == l {
                e += text[e..].chars().next().map_or(1, char::len_utf8);
            }
            let w = if s == start && e == end { f.w } else { m.width(f.font, &text[s..e]) };
            pieces.push((Frag { start: s as u32, end: e as u32, w, rtl: l % 2 == 1, ..f }, l));
            s = e;
        }
    }
    let levels: Vec<Level> = pieces.iter().map(|p| Level::new(p.1).unwrap_or_else(|_| Level::ltr())).collect();
    let order = BidiInfo::reorder_visual(&levels);
    let mut x = 0.0f32;
    for i in order {
        let mut f = pieces[i].0;
        f.x = x;
        x += f.w;
        out.frags.push(f);
    }
}

/// Bytes at the start where both texts and their runs agree.
fn common_prefix(a: &Inline, b: &Inline) -> usize {
    let (ta, tb) = (a.text().as_bytes(), b.text().as_bytes());
    let mut n = ta.iter().zip(tb).take_while(|(x, y)| x == y).count();
    for (ra, rb) in a.runs().iter().zip(b.runs()) {
        if ra.range.start >= n {
            break;
        }
        if ra.style != rb.style || ra.range.start != rb.range.start {
            n = n.min(ra.range.start.min(rb.range.start));
            break;
        }
        if ra.range.end != rb.range.end {
            n = n.min(ra.range.end.min(rb.range.end));
            break;
        }
    }
    while n > 0 && !b.text().is_char_boundary(n) {
        n -= 1;
    }
    n
}

fn layout_table(b: &Block, t: &Table, out: &mut Layout, ty: &Typo, m: &mut dyn Metrics) {
    let text = b.text();
    // Right to left (the table's first strong character): the first column
    // on the right and the table at the right edge.
    let rtl = out.rtl;
    let ncols = t.rows.iter().map(Vec::len).max().unwrap_or(0).max(1);
    let pad = ty.cell_pad;
    let avail = out.box_w;
    let base = FontKey {
        size: ty.body,
        ..Default::default()
    };
    let font_of = |ri: usize| FontKey {
        bold: ri < t.header_rows,
        ..base
    };
    // Natural (one line) width of each column.
    let mut natural = vec![0f32; ncols];
    for (ri, row) in t.rows.iter().enumerate() {
        for (ci, cell) in row.iter().enumerate() {
            let w = runs_width(text, b.content.runs(), cell.clone(), font_of(ri), m);
            natural[ci] = natural[ci].max(w.ceil() + 2.0 * pad as f32);
        }
    }
    let total: f32 = natural.iter().sum();
    let widths: Vec<i32> = if total <= avail as f32 {
        natural.iter().map(|w| w.ceil() as i32).collect()
    } else {
        // Every column gets a fair share; the rest goes to the wide ones.
        let fair = avail as f32 / ncols as f32;
        let base_w: Vec<f32> = natural.iter().map(|w| w.min(fair)).collect();
        let extra = avail as f32 - base_w.iter().sum::<f32>();
        let want: f32 = natural.iter().zip(&base_w).map(|(n, b)| n - b).sum();
        natural
            .iter()
            .zip(&base_w)
            .map(|(n, bw)| {
                (bw + if want > 0.0 {
                    extra * (n - bw) / want
                } else {
                    0.0
                })
                .floor()
                .max(2.0 * pad as f32 + 8.0) as i32
            })
            .collect()
    };
    let mut cols = Vec::with_capacity(ncols);
    let mut x = out.left;
    for w in &widths {
        cols.push((x, *w));
        x += w;
    }
    let total = x - out.left;
    if rtl {
        let right = out.left + avail;
        for c in cols.iter_mut() {
            c.0 = right - (c.0 - out.left) - c.1;
        }
        out.left = right - total;
    }
    let mut rows = Vec::with_capacity(t.rows.len());
    let mut y = 0;
    for (ri, row) in t.rows.iter().enumerate() {
        let mut row_h = ty.line_h(ty.body);
        let first_line = out.lines.len();
        for (ci, cell) in row.iter().enumerate() {
            let (cx, cw) = cols[ci];
            let start = out.lines.len();
            // Each cell reads in its own direction ("12 KB" stays left to
            // right in an Arabic table); a cell without letters takes the table's.
            let part = &text[cell.clone()];
            let cell_rtl = match unicode_bidi::get_base_direction_full(part) {
                unicode_bidi::Direction::Rtl => true,
                unicode_bidi::Direction::Ltr => false,
                _ => rtl,
            };
            let bidi = Bidi::new(text, cell.start, cell.clone(), cell_rtl, false);
            let end_y = layout_text_in(
                text,
                b.content.runs(),
                cell.clone(),
                font_of(ri),
                cx + pad,
                cw - 2 * pad,
                y + pad,
                ty,
                m,
                out,
                &bidi,
            );
            row_h = row_h.max(end_y - y - pad);
            // Column alignment, in the table's direction: right to left,
            // Left is the start (the right edge) and Right the end.
            let align = t.align.get(ci).copied().unwrap_or_default();
            for li in start..out.lines.len() {
                let l = &out.lines[li];
                let used = out.frags_of(l).last().map_or(0.0, |f| f.x + f.w);
                let free = (l.w as f32 - used).max(0.0).floor() as i32;
                let at = match (align, rtl) {
                    (ColumnAlign::Center, _) => free / 2,
                    (ColumnAlign::Left, false) | (ColumnAlign::Right, true) => 0,
                    _ => free,
                };
                let now = if l.rtl { free } else { 0 };
                out.lines[li].x += at - now;
            }
        }
        let _ = first_line;
        let h = row_h + 2 * pad;
        rows.push((y, h));
        y += h;
    }
    out.height = y + 1;
    out.box_w = total.max(1);
    out.table = Some(TableGeom {
        cols,
        rows,
        header_rows: t.header_rows,
    });
}

fn runs_width(
    text: &str,
    runs: &[Run],
    range: Range<usize>,
    base: FontKey,
    m: &mut dyn Metrics,
) -> f32 {
    let mut w = 0.0;
    for r in runs {
        let s = r.range.start.max(range.start);
        let e = r.range.end.min(range.end);
        if s < e {
            w += m.width(base.with(&r.style), &text[s..e]);
        }
    }
    w
}

// ---------------------------------------------------------- line breaking

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Word,
    Cjk,
    Space,
    Tab,
    Newline,
}

#[derive(Debug, Clone, Copy)]
struct Tok {
    start: usize,
    end: usize,
    kind: Kind,
    run: u32,
    font: FontKey,
    w: f32,
}

/// Ideographs, kana, hangul and fullwidth forms: a line may break between
/// any two of them.
pub fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF | 0x2E80..=0x2FFF | 0x3000..=0x303F | 0x3040..=0x30FF | 0x3100..=0x31FF | 0x3200..=0x4DBF
        | 0x4E00..=0x9FFF | 0xA960..=0xA97F | 0xAC00..=0xD7FF | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFFEF
        | 0x20000..=0x3FFFF)
}

/// No line starts with these (closing punctuation, small kana marks).
fn no_break_before(c: char) -> bool {
    "、。，．・：；？！）」』】〉》〕｝］ー々ぁぃぅぇぉっゃゅょァィゥェォッャュョ%)]}.,;:!?"
        .contains(c)
}

/// No line ends with these (opening brackets).
fn no_break_after(c: char) -> bool {
    "（「『【〈《〔｛［([{".contains(c)
}

fn class(c: char) -> Kind {
    match c {
        '\n' => Kind::Newline,
        '\t' => Kind::Tab,
        '\u{a0}' | '\u{202f}' | '\u{2007}' => Kind::Word,
        c if c.is_whitespace() => Kind::Space,
        c if is_cjk(c) => Kind::Cjk,
        _ => Kind::Word,
    }
}

fn tokens(
    text: &str,
    runs: &[Run],
    range: Range<usize>,
    base: FontKey,
    m: &mut dyn Metrics,
    space_w: &mut Vec<(FontKey, f32)>,
) -> Vec<Tok> {
    let mut out: Vec<Tok> = Vec::new();
    let first = runs.partition_point(|r| r.range.end <= range.start);
    for (ri, r) in runs.iter().enumerate().skip(first) {
        let s = r.range.start.max(range.start);
        let e = r.range.end.min(range.end);
        if s >= e {
            if r.range.start >= range.end {
                break;
            }
            continue;
        }
        let font = base.with(&r.style);
        let mut cur: Option<Tok> = None;
        for (i, c) in text[s..e].char_indices() {
            let at = s + i;
            let k = class(c);
            let joins = match (&cur, k) {
                (Some(t), Kind::Word) => t.kind == Kind::Word && !text[t.start..at].ends_with('-'),
                (Some(t), Kind::Space) => t.kind == Kind::Space,
                _ => false,
            };
            // A hyphen ends a word only when a letter follows.
            let joins = joins
                || (k == Kind::Word
                    && cur
                        .as_ref()
                        .is_some_and(|t| t.kind == Kind::Word && text[t.start..at] == *"-"));
            if joins {
                if let Some(t) = cur.as_mut() {
                    t.end = at + c.len_utf8();
                }
                continue;
            }
            if let Some(t) = cur.take() {
                out.push(t);
            }
            cur = Some(Tok {
                start: at,
                end: at + c.len_utf8(),
                kind: k,
                run: ri as u32,
                font,
                w: 0.0,
            });
        }
        if let Some(t) = cur.take() {
            out.push(t);
        }
    }
    for t in out.iter_mut() {
        t.w = match t.kind {
            Kind::Newline | Kind::Tab => 0.0,
            Kind::Space if text[t.start..t.end].bytes().all(|b| b == b' ') => {
                let sw = match space_w.iter().find(|(f, _)| *f == t.font) {
                    Some((_, w)) => *w,
                    None => {
                        let w = m.width(t.font, " ");
                        space_w.push((t.font, w));
                        w
                    }
                };
                sw * (t.end - t.start) as f32
            }
            _ => m.width(t.font, &text[t.start..t.end]),
        };
    }
    out
}

/// May the line break between `a` and `b`?
fn may_break(text: &str, a: &Tok, b: &Tok) -> bool {
    let last = text[a.start..a.end].chars().next_back().unwrap_or(' ');
    let next = text[b.start..b.end].chars().next().unwrap_or(' ');
    match (a.kind, b.kind) {
        (Kind::Space | Kind::Tab, Kind::Space | Kind::Tab) => false,
        (Kind::Space | Kind::Tab, _) => true,
        (_, Kind::Space | Kind::Tab | Kind::Newline) => false,
        (Kind::Cjk, _) | (_, Kind::Cjk) => !no_break_before(next) && !no_break_after(last),
        (Kind::Word, Kind::Word) => {
            last == '-' && text[a.start..a.end].len() > 1 && next.is_alphanumeric()
        }
        _ => false,
    }
}

struct Placed {
    tok: Tok,
    x: f32,
}

/// Lays out bytes `range` of `text` into lines at (x, y), `w` wide; returns
/// the y below the last line. The base direction is the first strong
/// character of `range` (see the module docs).
#[allow(clippy::too_many_arguments)]
pub fn layout_text(
    text: &str,
    runs: &[Run],
    range: Range<usize>,
    base: FontKey,
    x: i32,
    w: i32,
    y: i32,
    ty: &Typo,
    m: &mut dyn Metrics,
    out: &mut Layout,
) -> i32 {
    let part = &text[range.clone()];
    let rtl = has_rtl(part) && unicode_bidi::get_base_direction_full(part) == unicode_bidi::Direction::Rtl;
    let bidi = Bidi::new(text, range.start, range.clone(), rtl, false);
    layout_text_in(text, runs, range, base, x, w, y, ty, m, out, &bidi)
}

#[allow(clippy::too_many_arguments)]
fn layout_text_in(
    text: &str,
    runs: &[Run],
    range: Range<usize>,
    base: FontKey,
    x: i32,
    w: i32,
    y: i32,
    ty: &Typo,
    m: &mut dyn Metrics,
    out: &mut Layout,
    bidi: &Bidi,
) -> i32 {
    let max = w.max(8) as f32 + 0.05;
    let mut space_w = Vec::new();
    let mut toks = tokens(text, runs, range.clone(), base, m, &mut space_w);
    let tab_w = 4.0 * m.width(FontKey { mono: true, ..base }, " ");
    let mut y = y;
    let mut line: Vec<Placed> = Vec::new();
    let mut line_start = range.start;
    let mut lx = 0.0f32;
    let mut i = 0;
    let n = toks.len();
    if n == 0 {
        // An empty block still has one line.
        finish_line(
            text, &mut line, line_start, range.end, base, x, w, &mut y, ty, m, out, bidi,
        );
        return y;
    }
    while i < n {
        // A segment: tokens up to the next break opportunity, then its blanks.
        if toks[i].kind == Kind::Newline {
            let end = toks[i].end;
            finish_line(
                text, &mut line, line_start, end, base, x, w, &mut y, ty, m, out, bidi,
            );
            line_start = end;
            lx = 0.0;
            i += 1;
            if i == n {
                // Text ending in a newline: the empty last line is shown.
                finish_line(
                    text, &mut line, line_start, range.end, base, x, w, &mut y, ty, m, out, bidi,
                );
            }
            continue;
        }
        let mut j = i + 1;
        while j < n && toks[j].kind != Kind::Newline && !may_break(text, &toks[j - 1], &toks[j]) {
            j += 1;
        }
        // Body (without trailing blanks) of toks[i..j].
        let mut body_end = j;
        while body_end > i && matches!(toks[body_end - 1].kind, Kind::Space | Kind::Tab) {
            body_end -= 1;
        }
        let body_w: f32 = toks[i..body_end].iter().map(|t| t.w).sum();
        if lx + body_w > max && !line.is_empty() {
            let end = toks[i].start;
            finish_line(
                text, &mut line, line_start, end, base, x, w, &mut y, ty, m, out, bidi,
            );
            line_start = end;
            lx = 0.0;
        }
        if body_w > max && line.is_empty() {
            // Too wide for any line: break inside, at grapheme boundaries.
            let mut k = i;
            while k < body_end {
                let t = toks[k];
                if lx + t.w <= max {
                    line.push(Placed { tok: t, x: lx });
                    lx += t.w;
                    k += 1;
                    continue;
                }
                let (fit, fit_w) = fit_prefix(text, &t, max - lx, line.is_empty(), m);
                if fit > t.start {
                    line.push(Placed {
                        tok: Tok {
                            end: fit,
                            w: fit_w,
                            ..t
                        },
                        x: lx,
                    });
                }
                let brk = fit.max(t.start);
                finish_line(
                    text, &mut line, line_start, brk, base, x, w, &mut y, ty, m, out, bidi,
                );
                line_start = brk;
                lx = 0.0;
                if fit >= t.end {
                    k += 1;
                } else {
                    toks[k] = Tok {
                        start: brk,
                        w: m.width(t.font, &text[brk..t.end]),
                        ..t
                    };
                }
            }
            for t in &toks[body_end..j] {
                place_blank(&mut line, t, &mut lx, tab_w);
            }
            i = j;
            continue;
        }
        for t in &toks[i..body_end] {
            line.push(Placed { tok: *t, x: lx });
            lx += t.w;
        }
        for t in &toks[body_end..j] {
            place_blank(&mut line, t, &mut lx, tab_w);
        }
        i = j;
    }
    if !line.is_empty() || line_start < range.end {
        finish_line(
            text, &mut line, line_start, range.end, base, x, w, &mut y, ty, m, out, bidi,
        );
    }
    y
}

fn place_blank(line: &mut Vec<Placed>, t: &Tok, lx: &mut f32, tab_w: f32) {
    let w = if t.kind == Kind::Tab && tab_w > 0.0 {
        ((*lx / tab_w).floor() + 1.0) * tab_w - *lx
    } else {
        t.w
    };
    line.push(Placed {
        tok: Tok { w, ..*t },
        x: *lx,
    });
    *lx += w;
}

/// The longest grapheme prefix of `t` that fits `room` (at least one
/// grapheme on an empty line). Returns (end byte, width).
fn fit_prefix(
    text: &str,
    t: &Tok,
    room: f32,
    empty_line: bool,
    m: &mut dyn Metrics,
) -> (usize, f32) {
    let s = &text[t.start..t.end];
    let bounds: Vec<usize> = s.grapheme_indices(true).map(|(i, g)| i + g.len()).collect();
    let (mut lo, mut hi) = (0usize, bounds.len());
    let mut best = (0usize, 0.0f32);
    // Binary search on the number of graphemes that fit.
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let wm = m.width(t.font, &s[..bounds[mid - 1]]);
        if wm <= room {
            best = (mid, wm);
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    if best.0 == 0 && empty_line && !bounds.is_empty() {
        best = (1, m.width(t.font, &s[..bounds[0]]));
    }
    if best.0 == 0 {
        return (t.start, 0.0);
    }
    (t.start + bounds[best.0 - 1], best.1)
}

#[allow(clippy::too_many_arguments)]
fn finish_line(
    text: &str,
    line: &mut Vec<Placed>,
    start: usize,
    end: usize,
    base: FontKey,
    x: i32,
    w: i32,
    y: &mut i32,
    ty: &Typo,
    m: &mut dyn Metrics,
    out: &mut Layout,
    bidi: &Bidi,
) {
    let f0 = out.frags.len() as u32;
    // Trailing blanks are not drawn.
    let mut drawn = line.len();
    while drawn > 0 && line[drawn - 1].tok.kind == Kind::Space {
        drawn -= 1;
    }
    let mut asc = 0;
    let mut desc = 0;
    let mut lh = 0;
    let mut fonts: Vec<FontKey> = Vec::new();
    for (pi, p) in line.iter().enumerate() {
        let t = &p.tok;
        if !fonts.contains(&t.font) {
            fonts.push(t.font);
        }
        if pi >= drawn {
            continue;
        }
        let tab = t.kind == Kind::Tab;
        let mergeable = out.frags.len() as u32 > f0;
        match out.frags.last_mut() {
            Some(f)
                if mergeable && !tab && !f.tab && f.run == t.run && f.end as usize == t.start =>
            {
                f.end = t.end as u32;
                f.w = p.x + t.w - f.x;
            }
            _ => out.frags.push(Frag {
                x: p.x,
                w: t.w,
                start: t.start as u32,
                end: t.end as u32,
                run: t.run,
                font: t.font,
                tab,
                rtl: false,
            }),
        }
    }
    if bidi.on && out.frags.len() > f0 as usize {
        reorder_line(text, out, f0 as usize, bidi, m);
    }
    // Right to left: right-aligned.
    let x = if bidi.rtl {
        let used = out.frags[f0 as usize..].last().map_or(0.0, |f| f.x + f.w);
        x + (w as f32 - used).max(0.0).floor() as i32
    } else {
        x
    };
    if fonts.is_empty() {
        fonts.push(base);
    }
    for f in fonts {
        let (a, d) = m.vmetrics(f);
        asc = asc.max(a);
        desc = desc.max(d);
        lh = lh.max(ty.line_h(f.size));
    }
    let lh = lh.max(asc + desc);
    let basel = (lh - (asc + desc)) / 2 + asc;
    out.lines.push(Line {
        x,
        y: *y,
        w,
        h: lh,
        base: basel,
        start: start as u32,
        end: end as u32,
        frags: f0..out.frags.len() as u32,
        rtl: bidi.rtl,
    });
    *y += lh;
    line.clear();
}

/// Height of `b` without laying it out (blocks out of view), from the
/// average char width of the theme font.
pub fn estimate(b: &Block, width: i32, ty: &Typo, avg_char: f32) -> i32 {
    estimate_of(Shape::of(b), b.text(), width, ty, avg_char)
}

pub(crate) fn estimate_of(s: Shape, text: &str, width: i32, ty: &Typo, avg_char: f32) -> i32 {
    let width = width.max(40);
    let base = ty.base_of(s);
    let lh = ty.line_h(base.size);
    let cw = (avg_char * base.size as f32 / ty.body.max(1) as f32).max(1.0);
    let avail = (width - ty.left_of(s)) as f32
        - if s.is_code() {
            2.0 * ty.code_pad as f32
        } else {
            0.0
        };
    let cols = (avail / cw).max(1.0);
    let lines = |s: &str| -> i32 {
        s.split('\n')
            .map(|l| ((l.chars().count() as f32 / cols).ceil() as i32).max(1))
            .sum()
    };
    match s.kind {
        ShapeKind::Rule => ty.gap,
        ShapeKind::Table(rows) => rows as i32 * (ty.line_h(ty.body) + 2 * ty.cell_pad) + 1,
        ShapeKind::Code => ty.code_header + lines(text) * lh + ty.code_pad,
        ShapeKind::Heading(l) => lines(text) * lh + if l <= 2 { 2 * ty.item_gap + 1 } else { 0 },
        ShapeKind::Paragraph => lines(text) * lh,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typo() -> Typo {
        Typo {
            body: 10,
            small: 8,
            heading: [16, 14, 12, 10, 10, 9],
            line: 1.5,
            gap: 8,
            item_gap: 2,
            list_w: 20,
            quote_w: 12,
            code_pad: 6,
            code_header: 14,
            cell_pad: 4,
            family: "x".into(),
        }
    }

    fn lines(b: &Block, width: i32) -> Vec<String> {
        let l = layout_block(b, width, &typo(), &mut FakeMetrics, None);
        l.lines
            .iter()
            .map(|ln| b.text()[ln.start as usize..ln.end as usize].to_string())
            .collect()
    }

    #[test]
    fn wraps_at_spaces_and_hyphens_and_hangs_blanks() {
        // 6 px per char at size 10: 60 px = 10 chars.
        let b = Block::paragraph("hello world, a well-known fact");
        assert_eq!(
            lines(&b, 60),
            vec!["hello ", "world, a ", "well-known ", "fact"]
        );
        assert_eq!(
            lines(&b, 54),
            vec!["hello ", "world, a ", "well-", "known ", "fact"]
        );
        let l = layout_block(&b, 60, &typo(), &mut FakeMetrics, None);
        // Lines tile the text; blanks at line ends are not drawn.
        assert_eq!(l.lines.last().unwrap().end as usize, b.text().len());
        let f = &l.frags[l.lines[0].frags.start as usize];
        assert_eq!(&b.text()[f.start as usize..f.end as usize], "hello");
        assert_eq!(l.height, 4 * 15);
    }

    #[test]
    fn long_words_break_at_graphemes_and_cjk_anywhere() {
        let b = Block::paragraph("abcdefghijklmnopqrstuvwxyz");
        assert_eq!(lines(&b, 60), vec!["abcdefghij", "klmnopqrst", "uvwxyz"]);
        // A family emoji (one grapheme of 7 chars) is never split.
        let fam = "👩\u{200d}👩\u{200d}👧";
        let b = Block::paragraph(&format!("{fam}{fam}{fam}"));
        for l in lines(&b, 48) {
            assert!(l.len() % fam.len() == 0, "split inside a grapheme: {l:?}");
        }
        // CJK: 12 px each; 60 px = 5 per line; no line starts with 。
        let b = Block::paragraph("日本語のテキストです。とても長い");
        let ls = lines(&b, 60);
        assert!(ls.len() >= 3);
        assert!(ls.iter().all(|l| !l.starts_with('。')), "{ls:?}");
        assert_eq!(ls.concat(), b.text());
        // Arabic goes through the same path (one fragment per run and line).
        let b = Block::paragraph("مرحبا بالعالم هذا نص عربي طويل");
        assert_eq!(lines(&b, 60).concat(), b.text());
    }

    #[test]
    fn styled_runs_make_fragments_and_hard_breaks_lines() {
        let b = Block::paragraph("plain ")
            .push("bold", &Style::plain().bold())
            .push(" code\nnext", &Style::plain());
        let l = layout_block(&b, 400, &typo(), &mut FakeMetrics, None);
        assert_eq!(l.lines.len(), 2);
        let frags = l.frags_of(&l.lines[0]);
        assert_eq!(frags.len(), 3);
        assert!(frags[1].font.bold && !frags[0].font.bold);
        assert!(frags[1].x > frags[0].x + frags[0].w - 0.01);
    }

    #[test]
    fn code_tabs_and_empty_lines() {
        let b = Block::code("", "a\tb\n\n\tc");
        let l = layout_block(&b, 400, &typo(), &mut FakeMetrics, None);
        assert_eq!(l.lines.len(), 3);
        let f = l.frags_of(&l.lines[0]);
        // Tab stop at 4 spaces (24 px): "b" starts there.
        assert!(f[1].tab && (f[2].x - 24.0).abs() < 0.01, "{f:?}");
        assert_eq!(l.lines[0].x, typo().code_pad);
        assert_eq!(l.lines[0].y, typo().code_header);
    }

    #[test]
    fn incremental_relayout_equals_full_layout() {
        let ty = typo();
        let mut text = String::new();
        let mut prev: Option<(Block, Layout)> = None;
        for i in 0..60 {
            text.push_str(&format!("word{i} "));
            if i % 17 == 0 {
                text.push('\n');
            }
            let b = Block::code("rust", &text);
            let full = layout_block(&b, 120, &ty, &mut FakeMetrics, None);
            let inc = layout_block(
                &b,
                120,
                &ty,
                &mut FakeMetrics,
                prev.as_ref().map(|(b, l)| (b, l)),
            );
            assert_eq!(inc, full, "step {i}");
            prev = Some((b, inc));
        }
    }

    #[test]
    fn tables_hit_testing_and_selection_rects() {
        let b = Block::table(
            &[ColumnAlign::Left, ColumnAlign::Right],
            &[&["name", "size"], &["a", "12345"]],
            true,
        );
        let l = layout_block(&b, 400, &typo(), &mut FakeMetrics, None);
        let g = l.table.as_ref().unwrap();
        assert_eq!(g.cols.len(), 2);
        assert_eq!(g.rows.len(), 2);
        assert_eq!(l.lines.len(), 4);
        // Right aligned: the short header "size" moves right.
        let BlockKind::Table(t) = &b.kind else {
            panic!()
        };
        assert!(l.lines[1].x > g.cols[1].0 + typo().cell_pad - 1);
        let p = Block::paragraph("hello world");
        let pl = layout_block(&p, 400, &typo(), &mut FakeMetrics, None);
        assert_eq!(pl.hit(p.text(), 0, 3, &mut FakeMetrics), 0);
        assert_eq!(pl.hit(p.text(), 13, 3, &mut FakeMetrics), 2);
        assert_eq!(pl.hit(p.text(), 999, 3, &mut FakeMetrics), 11);
        let r = pl.rects(p.text(), 6..11, &mut FakeMetrics);
        assert_eq!(r, vec![(36, 0, 30, 15)]);
        let cell = &t.rows[1][1];
        assert_eq!(&b.text()[cell.clone()], "12345");
    }

    /// Texts of the fragments of line `li`, left to right on screen.
    fn visual(b: &Block, l: &Layout, li: usize) -> Vec<String> {
        l.frags_of(&l.lines[li])
            .iter()
            .map(|f| b.text()[f.start as usize..f.end as usize].to_string())
            .collect()
    }

    #[test]
    fn bidi_rtl_paragraph_reorders_and_right_aligns() {
        // Base direction from the first strong character (Arabic).
        let b = Block::paragraph("مرحبا world 123");
        let l = layout_block(&b, 400, &typo(), &mut FakeMetrics, None);
        assert!(l.rtl && l.lines[0].rtl);
        // W7: the number follows "world" (L), so "world 123" is one run.
        assert_eq!(visual(&b, &l, 0), vec!["world 123", "مرحبا "]);
        let f = l.frags_of(&l.lines[0]);
        assert!(f[0].x == 0.0 && f.windows(2).all(|w| (w[0].x + w[0].w - w[1].x).abs() < 0.01));
        assert!(f[1].rtl && !f[0].rtl);
        // A number after Arabic stays after it, read left to right.
        let b2 = Block::paragraph("السعر 120 دولار");
        let l2 = layout_block(&b2, 400, &typo(), &mut FakeMetrics, None);
        assert_eq!(visual(&b2, &l2, 0), vec![" دولار", "120", "السعر "]);
        // Right-aligned: the line ends at the column's right edge.
        let ln = &l.lines[0];
        let used = f.last().map(|f| f.x + f.w).unwrap();
        assert!((ln.x as f32 + used - 400.0).abs() <= 1.0, "x {} used {used}", ln.x);
        // Hit testing: the right end is the logical start, the left end the logical end.
        let text = b.text();
        assert_eq!(l.hit(text, 399, 3, &mut FakeMetrics), 0);
        assert_eq!(l.hit(text, ln.x - 5, 3, &mut FakeMetrics), text.len());
        // The caret before the first Arabic letter is at the right edge.
        assert!((l.x_of(text, 0, 0, &mut FakeMetrics) - 400.0).abs() <= 1.0);
        // Byte 2 (after the first letter): one char (6 px) left of the edge.
        assert!((l.x_of(text, 0, 2, &mut FakeMetrics) - 394.0).abs() <= 1.0);
        // "world" reads left to right inside the right-to-left line.
        let w0 = text.find("world").unwrap();
        let (a, z) = l.span_x(text, 0, w0..w0 + 5, &mut FakeMetrics);
        assert!((z - a - 30.0).abs() < 0.01);
        assert!(l.x_of(text, 0, w0, &mut FakeMetrics) < l.x_of(text, 0, w0 + 5, &mut FakeMetrics));
        // A hit in the middle of "world" maps back to its bytes.
        let mid = l.hit(text, (a + 13.0) as i32, 3, &mut FakeMetrics);
        assert_eq!(mid, w0 + 2);
    }

    #[test]
    fn bidi_selection_covers_separate_spans_in_mixed_lines() {
        // Left to right paragraph with an Arabic phrase: selecting from the
        // middle of the phrase to the end gives two pieces on screen.
        let b = Block::paragraph("abc مرحبا بالعالم xyz");
        let l = layout_block(&b, 400, &typo(), &mut FakeMetrics, None);
        assert!(!l.rtl);
        assert_eq!(visual(&b, &l, 0), vec!["abc ", "مرحبا بالعالم", " xyz"]);
        let text = b.text();
        let from = text.find("بالعالم").unwrap();
        let r = l.rects(text, from..text.len(), &mut FakeMetrics);
        // "بالعالم" is drawn at the left end of the Arabic phrase, " xyz" after it:
        // not adjacent on screen, so two rectangles.
        assert_eq!(r.len(), 2, "{r:?}");
        let arabic = l.frags_of(&l.lines[0])[1];
        assert_eq!(r[0].0, arabic.x as i32);
        assert!(r[0].2 > 0 && r[1].0 >= (arabic.x + arabic.w) as i32);
    }

    #[test]
    fn bidi_styled_runs_and_ltr_text_are_untouched() {
        // No right-to-left characters: the fast path, as before.
        let b = Block::paragraph("plain ").push("bold", &Style::plain().bold());
        let l = layout_block(&b, 400, &typo(), &mut FakeMetrics, None);
        assert!(!l.rtl && l.frags.iter().all(|f| !f.rtl));
        assert_eq!(l.frags_of(&l.lines[0])[0].x, 0.0);
        // Styled Arabic runs move as a whole: bold word first on the right.
        let b = Block::paragraph("").push("عريض", &Style::plain().bold()).push(" عادي", &Style::plain());
        let l = layout_block(&b, 400, &typo(), &mut FakeMetrics, None);
        assert_eq!(visual(&b, &l, 0), vec![" عادي", "عريض"]);
        assert!(l.frags_of(&l.lines[0])[1].font.bold);
        // An explicit RLM makes a Latin paragraph right to left.
        let b = Block::paragraph("\u{200f}ok 42");
        let l = layout_block(&b, 200, &typo(), &mut FakeMetrics, None);
        assert!(l.rtl && l.lines[0].x > 100);
        // Code stays left to right.
        let b = Block::code("", "مرحبا = 1");
        let l = layout_block(&b, 400, &typo(), &mut FakeMetrics, None);
        assert!(!l.rtl && l.lines[0].x == typo().code_pad);
    }

    #[test]
    fn bidi_wrapped_lines_and_indentation_mirror() {
        let b = Block::paragraph("واحد اثنان ثلاثة أربعة خمسة ستة سبعة").item(ListMarker::Bullet, 1);
        let l = layout_block(&b, 120, &typo(), &mut FakeMetrics, None);
        assert!(l.lines.len() >= 3);
        for ln in &l.lines {
            let used = l.frags_of(ln).last().map_or(0.0, |f| f.x + f.w);
            // Indentation on the right: lines end list_w before the right edge.
            assert!((ln.x as f32 + used - (120 - typo().list_w) as f32).abs() <= 1.0, "{ln:?}");
        }
        assert_eq!(l.mirror_x(l.left - typo().list_w, typo().list_w), 120 - typo().list_w);
        let joined: String = l.lines.iter().map(|ln| &b.text()[ln.start as usize..ln.end as usize]).collect();
        assert_eq!(joined, b.text());
    }

    #[test]
    fn bidi_incremental_relayout_equals_full_layout() {
        let ty = typo();
        let mut text = String::new();
        let mut prev: Option<(Block, Layout)> = None;
        for i in 0..60 {
            text.push_str(if i % 3 == 0 { "كلمة " } else if i % 3 == 1 { "word " } else { "123 " });
            if i % 17 == 0 {
                text.push('\n');
            }
            let b = Block::paragraph(&text);
            let full = layout_block(&b, 120, &ty, &mut FakeMetrics, None);
            let inc = layout_block(&b, 120, &ty, &mut FakeMetrics, prev.as_ref().map(|(b, l)| (b, l)));
            assert_eq!(inc, full, "step {i}");
            assert!(full.rtl);
            prev = Some((b, inc));
        }
    }

    #[test]
    fn bidi_ltr_cell_in_rtl_table_keeps_its_order() {
        let b = Block::table(&[ColumnAlign::Left, ColumnAlign::Right], &[&["الملف", "الحجم"], &["notes.md", "12 KB"]], true);
        let l = layout_block(&b, 400, &typo(), &mut FakeMetrics, None);
        let text = b.text();
        let cell = l.lines.iter().find(|ln| text[ln.start as usize..ln.end as usize].contains("KB")).expect("cell line");
        assert!(!cell.rtl, "an LTR cell is laid out left to right");
        let shown: String = l.frags_of(cell).iter().map(|f| &text[f.start as usize..f.end as usize]).collect();
        assert_eq!(shown.trim(), "12 KB");
        assert!(l.frags_of(cell).iter().all(|f| !f.rtl));
    }

    #[test]
    fn bidi_rtl_table_mirrors_columns() {
        let b = Block::table(
            &[ColumnAlign::Left, ColumnAlign::Right],
            &[&["الاسم", "الحجم"], &["ملف", "12"]],
            true,
        );
        let l = layout_block(&b, 400, &typo(), &mut FakeMetrics, None);
        assert!(l.rtl);
        let g = l.table.as_ref().unwrap();
        // First column on the right, the table against the right edge.
        assert!(g.cols[0].0 > g.cols[1].0);
        assert_eq!(g.cols[0].0 + g.cols[0].1, 400);
        assert_eq!(l.left, g.cols[1].0);
        // Start-aligned (Left) cells sit at their column's right edge.
        let ln = &l.lines[0];
        let used = l.frags_of(ln).last().map_or(0.0, |f| f.x + f.w);
        assert!((ln.x as f32 + used - (g.cols[0].0 + g.cols[0].1 - typo().cell_pad) as f32).abs() <= 1.0);
        // End-aligned (Right) cells at the left edge.
        let ln = &l.lines[3];
        assert_eq!(ln.x, g.cols[1].0 + typo().cell_pad);
    }

    #[test]
    fn estimate_is_close_for_prose() {
        let ty = typo();
        let b = Block::paragraph(&"lorem ipsum dolor sit amet ".repeat(20));
        let real = layout_block(&b, 300, &ty, &mut FakeMetrics, None).height;
        let est = estimate(&b, 300, &ty, 6.0);
        assert!((est - real).abs() <= real / 5, "estimate {est} vs {real}");
    }
}
