//! Line layout of an [`EditBuffer`](super::EditBuffer): one
//! [`text::layout::Layout`](crate::text::layout::Layout) per paragraph
//! (text between newlines), built with the same word wrap as `RichText`
//! (FLTK measurement, CJK breaks, grapheme-safe breaks inside long words).
//! An edit lays out again only the paragraphs it touched; widths of
//! repeated words come from a cache, so a 100 KB paragraph re-wraps in a
//! few milliseconds.

use super::buffer::{Change, Chip};
use crate::text::layout::{layout_text, FltkMetrics, FontKey, Frag, Layout, Line, Metrics, Typo};
use crate::text::{ColorRole, Run, Style};
use std::collections::HashMap;
use std::ops::Range;

/// [`Metrics`] that remembers the width of short strings (words, spaces).
#[derive(Default)]
pub struct CachedMetrics<M: Metrics + Default = FltkMetrics> {
    inner: M,
    cache: HashMap<FontKey, HashMap<Box<str>, f32>>,
    entries: usize,
}

impl<M: Metrics + Default> CachedMetrics<M> {
    pub fn new(inner: M) -> Self {
        CachedMetrics {
            inner,
            cache: HashMap::new(),
            entries: 0,
        }
    }

    /// Forgets every width (the theme font changed).
    pub fn clear(&mut self) {
        self.cache.clear();
        self.entries = 0;
    }
}

impl<M: Metrics + Default> Metrics for CachedMetrics<M> {
    fn width(&mut self, f: FontKey, s: &str) -> f32 {
        if s.len() > 48 {
            return self.inner.width(f, s);
        }
        if let Some(w) = self.cache.get(&f).and_then(|m| m.get(s)) {
            return *w;
        }
        let w = self.inner.width(f, s);
        if self.entries > 200_000 {
            self.clear();
        }
        self.entries += 1;
        self.cache.entry(f).or_default().insert(s.into(), w);
        w
    }

    fn vmetrics(&mut self, f: FontKey) -> (i32, i32) {
        self.inner.vmetrics(f)
    }
}

/// Sequential changes folded into one (in coordinates of the text before
/// the first and after the last).
pub fn merge_changes(changes: &[Change]) -> Option<Change> {
    let (first, rest) = changes.split_first()?;
    let (mut s, mut e_cur, mut e_orig) = (
        first.at as isize,
        (first.at + first.new_len) as isize,
        (first.at + first.old_len) as isize,
    );
    for c in rest {
        let (at, old, new) = (c.at as isize, c.old_len as isize, c.new_len as isize);
        let end_cur = e_cur.max(at + old);
        e_orig = end_cur - (e_cur - e_orig);
        e_cur = end_cur + new - old;
        s = s.min(at);
    }
    Some(Change {
        at: s as usize,
        old_len: (e_orig - s) as usize,
        new_len: (e_cur - s) as usize,
    })
}

/// One laid out paragraph.
#[derive(Debug, Clone, Default)]
pub struct Para {
    /// Byte offset of its first char in the whole text.
    pub start: usize,
    /// Bytes, without the '\n'.
    pub len: usize,
    pub y: i32,
    pub layout: Layout,
    /// Run index → chip (for drawing).
    pub chip_runs: Vec<bool>,
    dirty: bool,
    /// The layout before an edit inside this paragraph: (layout, chip runs,
    /// edit start, old length, new length), for the incremental path.
    prev: Option<(Layout, Vec<bool>, usize, usize, usize)>,
}

impl Para {
    pub fn h(&self) -> i32 {
        self.layout.height
    }
}

/// Where the caret is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaretRect {
    pub x: i32,
    pub y: i32,
    pub h: i32,
    /// Paragraph and line index.
    pub para: usize,
    pub line: usize,
}

/// The layout of a whole text (see the module docs).
pub struct EditLayout {
    pub width: i32,
    pub paras: Vec<Para>,
    pub height: i32,
    pub typo: Typo,
    pub base: FontKey,
    pub rtl: bool,
    /// One line's height (the font is the same everywhere).
    pub line_h: i32,
}

impl EditLayout {
    pub fn new(typo: Typo, base: FontKey) -> EditLayout {
        EditLayout {
            width: 0,
            paras: Vec::new(),
            height: 0,
            line_h: typo.line_h(base.size),
            typo,
            base,
            rtl: false,
        }
    }

    /// Paragraph runs: plain text, chips as their own runs.
    fn runs(p_start: usize, len: usize, chips: &[Chip]) -> (Vec<Run>, Vec<bool>) {
        let mut runs = Vec::new();
        let mut is_chip = Vec::new();
        let end = p_start + len;
        let first = chips.partition_point(|c| c.range.end <= p_start);
        let mut at = p_start;
        for c in &chips[first..] {
            if c.range.start >= end {
                break;
            }
            let (s, e) = (c.range.start.max(p_start), c.range.end.min(end));
            if s > at {
                runs.push(Run {
                    range: at - p_start..s - p_start,
                    style: Style::plain(),
                });
                is_chip.push(false);
            }
            if e > s {
                runs.push(Run {
                    range: s - p_start..e - p_start,
                    style: Style::plain().color(ColorRole::Accent),
                });
                is_chip.push(true);
            }
            at = e.max(at);
        }
        if end > at || runs.is_empty() {
            runs.push(Run {
                range: at - p_start..end - p_start,
                style: Style::plain(),
            });
            is_chip.push(false);
        }
        (runs, is_chip)
    }

    fn lay(&self, p: &mut Para, text: &str, chips: &[Chip], m: &mut dyn Metrics) {
        let s = &text[p.start..p.start + p.len];
        let (runs, chip_runs) = Self::runs(p.start, p.len, chips);
        let prev = p.prev.take();
        let mut out = match prev {
            Some((old, old_runs, at, old_len, new_len))
                if old_runs == chip_runs && old.lines.len() > 8 =>
            {
                self.lay_incremental(s, &runs, &old, at, old_len, new_len, m)
            }
            _ => {
                let mut out = Layout {
                    width: self.width,
                    ..Default::default()
                };
                layout_text(
                    s,
                    &runs,
                    0..p.len,
                    self.base,
                    0,
                    self.width.max(8),
                    0,
                    &self.typo,
                    m,
                    &mut out,
                );
                self.align(&mut out, 0);
                out
            }
        };
        out.height = out.lines.last().map_or(0, |l| l.y + l.h).max(self.line_h);
        out.box_w = self.width;
        out.width = self.width;
        p.layout = out;
        p.chip_runs = chip_runs;
        p.dirty = false;
    }

    /// Right-aligns lines from `from` on in RTL.
    fn align(&self, out: &mut Layout, from: usize) {
        if !self.rtl {
            return;
        }
        for l in out.lines[from..].iter_mut() {
            let natural = out.frags[l.frags.start as usize..l.frags.end as usize]
                .last()
                .map_or(0.0, |f| f.x + f.w);
            l.x = (self.width - natural.ceil() as i32).max(0);
        }
    }

    /// Re-wraps a long paragraph after an edit at `at` (old length
    /// `old_len`, now `new_len`): from the line before the edit until a new
    /// line starts where a (shifted) old line started after the edit. Greedy
    /// wrapping depends only on the text after a line start, so every line
    /// from there on is the old one, moved.
    #[allow(clippy::too_many_arguments)]
    fn lay_incremental(
        &self,
        s: &str,
        runs: &[Run],
        old: &Layout,
        at: usize,
        old_len: usize,
        new_len: usize,
        m: &mut dyn Metrics,
    ) -> Layout {
        let delta = new_len as isize - old_len as isize;
        let old_end = at + old_len;
        let shift = |x: u32| (x as isize + delta) as u32;
        let li = old
            .lines
            .partition_point(|l| (l.end as usize) <= at)
            .min(old.lines.len() - 1);
        let k = li.saturating_sub(1);
        let ls = old.lines[k].start as usize;
        let y0 = old.lines[k].y;
        // First old line that starts after the edit.
        let after = old.lines.partition_point(|l| (l.start as usize) <= old_end);
        let mut extra = 3usize;
        loop {
            let j = after + extra;
            let we = if j < old.lines.len() {
                (old.lines[j].start as isize + delta) as usize
            } else {
                s.len()
            };
            let mut win = Layout {
                width: self.width,
                ..Default::default()
            };
            layout_text(
                s,
                runs,
                ls..we,
                self.base,
                0,
                self.width.max(8),
                y0,
                &self.typo,
                m,
                &mut win,
            );
            // Convergence: a produced line (not the last one of the window)
            // starting at a shifted old line start after the edit.
            let mut meet = None;
            if we < s.len() {
                for (i, l) in win
                    .lines
                    .iter()
                    .enumerate()
                    .take(win.lines.len().saturating_sub(1))
                    .skip(1)
                {
                    let st = l.start as isize - delta;
                    if st > old_end as isize {
                        if let Ok(mi) = old.lines.binary_search_by_key(&(st as u32), |o| o.start) {
                            meet = Some((i, mi));
                            break;
                        }
                    }
                }
            }
            if meet.is_none() && we < s.len() {
                extra *= 4;
                continue;
            }
            let mut out = Layout {
                width: self.width,
                ..Default::default()
            };
            for l in &old.lines[..k] {
                let f0 = out.frags.len() as u32;
                out.frags.extend_from_slice(old.frags_of(l));
                out.lines.push(Line {
                    frags: f0..out.frags.len() as u32,
                    ..l.clone()
                });
            }
            let first_new = out.lines.len();
            let take = meet.map_or(win.lines.len(), |(i, _)| i);
            for l in &win.lines[..take] {
                let f0 = out.frags.len() as u32;
                out.frags.extend_from_slice(win.frags_of(l));
                out.lines.push(Line {
                    frags: f0..out.frags.len() as u32,
                    ..l.clone()
                });
            }
            self.align(&mut out, first_new);
            if let Some((_, mi)) = meet {
                let y = out.lines.last().map_or(y0, |l| l.y + l.h);
                let dy = y - old.lines[mi].y;
                for l in &old.lines[mi..] {
                    let f0 = out.frags.len() as u32;
                    out.frags.extend(old.frags_of(l).iter().map(|f| Frag {
                        start: shift(f.start),
                        end: shift(f.end),
                        ..*f
                    }));
                    out.lines.push(Line {
                        start: shift(l.start),
                        end: shift(l.end),
                        y: l.y + dy,
                        frags: f0..out.frags.len() as u32,
                        ..l.clone()
                    });
                }
            }
            return out;
        }
    }

    /// Lays out everything again (width, font or direction changed).
    pub fn rebuild(&mut self, text: &str, chips: &[Chip], width: i32, m: &mut dyn Metrics) {
        self.width = width;
        self.line_h = self.typo.line_h(self.base.size);
        let mut paras = Vec::new();
        let mut at = 0;
        for piece in text.split('\n') {
            paras.push(Para {
                start: at,
                len: piece.len(),
                dirty: true,
                ..Default::default()
            });
            at += piece.len() + 1;
        }
        for p in paras.iter_mut() {
            self.lay(p, text, chips, m);
        }
        self.paras = paras;
        self.place();
    }

    /// Applies the buffer's changes: only touched paragraphs are laid out.
    pub fn update(&mut self, text: &str, chips: &[Chip], changes: &[Change], m: &mut dyn Metrics) {
        if self.paras.is_empty() {
            self.rebuild(text, chips, self.width, m);
            return;
        }
        if let Some(c) = merge_changes(changes) {
            let i0 = self.para_index(c.at);
            let old_end = c.at + c.old_len;
            let i1 = self.para_index_from(old_end, i0);
            let start = self.paras[i0].start;
            let old_tail = self.paras[i1].start + self.paras[i1].len;
            let delta = c.new_len as isize - c.old_len as isize;
            let new_tail = (old_tail as isize + delta) as usize;
            let mut fresh = Vec::new();
            let mut at = start;
            for piece in text[start..new_tail.min(text.len())].split('\n') {
                fresh.push(Para {
                    start: at,
                    len: piece.len(),
                    dirty: true,
                    ..Default::default()
                });
                at += piece.len() + 1;
            }
            if i0 == i1 && fresh.len() == 1 {
                let p = &mut self.paras[i0];
                fresh[0].prev = Some((
                    std::mem::take(&mut p.layout),
                    std::mem::take(&mut p.chip_runs),
                    c.at - start,
                    c.old_len,
                    c.new_len,
                ));
            }
            let n = fresh.len();
            self.paras.splice(i0..=i1, fresh);
            for p in self.paras[i0 + n..].iter_mut() {
                p.start = (p.start as isize + delta) as usize;
            }
        }
        // Chips that changed with the text are in dirty paragraphs already.
        let mut paras = std::mem::take(&mut self.paras);
        for p in paras.iter_mut().filter(|p| p.dirty) {
            self.lay(p, text, chips, m);
        }
        self.paras = paras;
        self.place();
    }

    fn place(&mut self) {
        let mut y = 0;
        for p in self.paras.iter_mut() {
            p.y = y;
            y += p.layout.height;
        }
        self.height = y.max(self.line_h);
    }

    /// The paragraph holding byte `pos`.
    pub fn para_index(&self, pos: usize) -> usize {
        self.paras
            .partition_point(|p| p.start <= pos)
            .saturating_sub(1)
            .min(self.paras.len().saturating_sub(1))
    }

    fn para_index_from(&self, pos: usize, from: usize) -> usize {
        from + self.paras[from..]
            .partition_point(|p| p.start <= pos)
            .saturating_sub(1)
    }

    /// Number of visual lines.
    pub fn line_count(&self) -> usize {
        self.paras.iter().map(|p| p.layout.lines.len().max(1)).sum()
    }

    /// (paragraph, line) of `pos`. `upstream`: at a wrap point, the end of
    /// the upper line instead of the start of the lower one.
    pub fn line_of(&self, pos: usize, upstream: bool) -> (usize, usize) {
        let pi = self.para_index(pos);
        let p = &self.paras[pi];
        let rel = pos.saturating_sub(p.start).min(p.len);
        let lines = &p.layout.lines;
        if lines.is_empty() {
            return (pi, 0);
        }
        let mut li = lines
            .partition_point(|l| (l.end as usize) <= rel)
            .min(lines.len() - 1);
        if upstream && li > 0 && lines[li - 1].end as usize == rel {
            li -= 1;
        }
        (pi, li)
    }

    /// X of byte `rel` (paragraph-relative) on line `li`, trailing blanks included.
    pub fn x_at(&self, text: &str, pi: usize, li: usize, rel: usize, m: &mut dyn Metrics) -> f32 {
        let p = &self.paras[pi];
        let s = &text[p.start..p.start + p.len];
        let Some(l) = p.layout.lines.get(li) else {
            return 0.0;
        };
        let frags = p.layout.frags_of(l);
        for f in frags {
            if rel <= f.end as usize {
                if rel <= f.start as usize {
                    return l.x as f32 + f.x;
                }
                if f.tab {
                    return l.x as f32 + f.x + f.w;
                }
                return l.x as f32 + f.x + m.width(f.font, &s[f.start as usize..rel]);
            }
        }
        let (x0, from) = frags
            .last()
            .map_or((0.0, l.start as usize), |f| (f.x + f.w, f.end as usize));
        let to = rel.min(l.end as usize).max(from);
        let x = l.x as f32 + x0 + m.width(self.base, &s[from..to]);
        x.min(self.width as f32)
    }

    /// Caret geometry of `pos` (layout coordinates).
    pub fn caret(&self, text: &str, pos: usize, upstream: bool, m: &mut dyn Metrics) -> CaretRect {
        if self.paras.is_empty() {
            return CaretRect {
                x: if self.rtl { self.width } else { 0 },
                y: 0,
                h: self.line_h,
                para: 0,
                line: 0,
            };
        }
        let (pi, li) = self.line_of(pos, upstream);
        let p = &self.paras[pi];
        let rel = pos.saturating_sub(p.start).min(p.len);
        let (y, h) = p
            .layout
            .lines
            .get(li)
            .map_or((0, self.line_h), |l| (l.y, l.h));
        let x = self.x_at(text, pi, li, rel, m);
        CaretRect {
            x: x.round() as i32,
            y: p.y + y,
            h,
            para: pi,
            line: li,
        }
    }

    /// The byte under (x, y) and whether it is the end of a wrapped line.
    pub fn hit(&self, text: &str, x: i32, y: i32, m: &mut dyn Metrics) -> (usize, bool) {
        if self.paras.is_empty() {
            return (0, false);
        }
        let pi = self
            .paras
            .partition_point(|p| p.y + p.h() <= y)
            .min(self.paras.len() - 1);
        let p = &self.paras[pi];
        let lines = &p.layout.lines;
        if lines.is_empty() {
            return (p.start, false);
        }
        let ry = (y - p.y).max(0);
        let li = lines
            .partition_point(|l| l.y + l.h <= ry)
            .min(lines.len() - 1);
        self.hit_line(text, pi, li, x, m)
    }

    /// The byte nearest to `x` on one line.
    pub fn hit_line(
        &self,
        text: &str,
        pi: usize,
        li: usize,
        x: i32,
        m: &mut dyn Metrics,
    ) -> (usize, bool) {
        let p = &self.paras[pi];
        let s = &text[p.start..p.start + p.len];
        let l = &p.layout.lines[li];
        let rel = p
            .layout
            .hit(s, x, l.y + l.h / 2, m)
            .clamp(l.start as usize, l.end as usize);
        let last = li + 1 == p.layout.lines.len();
        let mut pos = rel;
        if !last && rel == l.end as usize {
            // Past the end of a wrapped line: before its trailing blank, or
            // at the wrap point drawn at the end of this line.
            let trail = s[l.start as usize..rel]
                .chars()
                .next_back()
                .filter(|c| c.is_whitespace());
            if let Some(c) = trail {
                pos = rel - c.len_utf8();
                return (p.start + pos, false);
            }
            return (p.start + pos, true);
        }
        (p.start + pos, false)
    }

    /// Byte range of a visual line (whole text offsets).
    pub fn line_range(&self, pi: usize, li: usize) -> Range<usize> {
        let p = &self.paras[pi];
        match p.layout.lines.get(li) {
            Some(l) => p.start + l.start as usize..p.start + l.end as usize,
            None => p.start..p.start + p.len,
        }
    }

    /// The line above / below (crossing paragraphs), or None at the edge.
    pub fn neighbour(&self, pi: usize, li: usize, down: bool) -> Option<(usize, usize)> {
        let n = self.paras[pi].layout.lines.len().max(1);
        if down {
            if li + 1 < n {
                Some((pi, li + 1))
            } else if pi + 1 < self.paras.len() {
                Some((pi + 1, 0))
            } else {
                None
            }
        } else if li > 0 {
            Some((pi, li - 1))
        } else if pi > 0 {
            Some((pi - 1, self.paras[pi - 1].layout.lines.len().max(1) - 1))
        } else {
            None
        }
    }

    /// Rectangles (layout coordinates) covering bytes `range`, only for
    /// lines between `y0` and `y1`.
    pub fn rects(
        &self,
        text: &str,
        range: Range<usize>,
        y0: i32,
        y1: i32,
        m: &mut dyn Metrics,
    ) -> Vec<(i32, i32, i32, i32)> {
        let mut out = Vec::new();
        if range.is_empty() || self.paras.is_empty() {
            return out;
        }
        let first = self.paras.partition_point(|p| p.y + p.h() <= y0);
        for pi in first..self.paras.len() {
            let p = &self.paras[pi];
            if p.y > y1 {
                break;
            }
            let pend = p.start + p.len;
            if range.end < p.start || range.start > pend {
                continue;
            }
            for (li, l) in p.layout.lines.iter().enumerate() {
                let (ls, le) = (p.start + l.start as usize, p.start + l.end as usize);
                if range.end < ls
                    || range.start > le
                    || (range.end == ls && ls < le)
                    || (range.start == le && le < pend)
                {
                    continue;
                }
                let s = range.start.max(ls) - p.start;
                let e = range.end.min(le) - p.start;
                let x0 = self.x_at(text, pi, li, s, m);
                let mut x1 = self.x_at(text, pi, li, e, m);
                // The selected newline shows as a bit of the line end.
                if range.end > le && li + 1 == p.layout.lines.len() {
                    x1 += (l.h / 3).max(3) as f32;
                }
                if x1 > x0 {
                    out.push((
                        x0.floor() as i32,
                        p.y + l.y,
                        (x1.ceil() - x0.floor()) as i32,
                        l.h,
                    ));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::super::buffer::EditBuffer;
    use super::*;
    use crate::text::layout::FakeMetrics;

    fn typo() -> Typo {
        Typo {
            body: 10,
            small: 8,
            heading: [10; 6],
            line: 1.4,
            gap: 0,
            item_gap: 0,
            list_w: 0,
            quote_w: 0,
            code_pad: 0,
            code_header: 0,
            cell_pad: 0,
            family: String::new(),
        }
    }

    fn lay(b: &EditBuffer, w: i32) -> EditLayout {
        let mut l = EditLayout::new(
            typo(),
            FontKey {
                size: 10,
                ..Default::default()
            },
        );
        l.rebuild(b.text(), b.chips(), w, &mut FakeMetrics);
        l
    }

    #[test]
    fn wraps_words_and_counts_lines() {
        // 6 px per char: 60 px fit 10 chars.
        let b = EditBuffer::new("hello brave new world\n\nend");
        let l = lay(&b, 60);
        assert_eq!(l.paras.len(), 3);
        assert_eq!(l.paras[0].layout.lines.len(), 3);
        assert_eq!(l.line_count(), 5);
        assert_eq!(l.height, 5 * l.line_h);
        // "hello " | "brave new " | "world"
        assert_eq!(l.line_range(0, 1), 6..16);
    }

    #[test]
    fn edits_relayout_only_touched_paragraphs() {
        let mut b = EditBuffer::new("aaa\nbbb\nccc");
        let mut l = lay(&b, 200);
        b.take_changes();
        let before = l.paras[2].layout.clone();
        b.move_to(1, false);
        b.insert("X\nY");
        let ch = b.take_changes();
        l.update(b.text(), b.chips(), &ch, &mut FakeMetrics);
        assert_eq!(l.paras.len(), 4);
        assert_eq!(
            l.paras.iter().map(|p| p.start).collect::<Vec<_>>(),
            vec![0, 3, 7, 11]
        );
        assert_eq!(l.paras[3].layout, before);
        // Same result as a full layout.
        let full = lay(&b, 200);
        assert_eq!(
            full.paras
                .iter()
                .map(|p| (p.start, p.len))
                .collect::<Vec<_>>(),
            l.paras.iter().map(|p| (p.start, p.len)).collect::<Vec<_>>()
        );
        b.select_all();
        b.insert("");
        let ch = b.take_changes();
        l.update(b.text(), b.chips(), &ch, &mut FakeMetrics);
        assert_eq!(l.paras.len(), 1);
        assert_eq!(l.line_count(), 1);
    }

    #[test]
    fn caret_and_hit_round_trip() {
        let b = EditBuffer::new("hello brave new world");
        let l = lay(&b, 60);
        let m = &mut FakeMetrics;
        let c = l.caret(b.text(), 8, false, m);
        assert_eq!((c.x, c.y), (12, l.line_h));
        assert_eq!(l.hit(b.text(), 12, l.line_h + 2, m), (8, false));
        // At the wrap point: start of the next line, or end of this one.
        let down = l.caret(b.text(), 6, false, m);
        let up = l.caret(b.text(), 6, true, m);
        assert_eq!((down.line, up.line), (1, 0));
        // Past the end of a wrapped line: before its trailing blank.
        assert_eq!(l.hit(b.text(), 59, 2, m), (5, false));
        // Trailing space at the very end moves the caret.
        let b2 = EditBuffer::new("ab ");
        let l2 = lay(&b2, 60);
        assert!(l2.caret(b2.text(), 3, false, m).x > l2.caret(b2.text(), 2, false, m).x);
    }

    #[test]
    fn chips_are_their_own_runs_and_rtl_aligns_right() {
        let mut b = EditBuffer::new("see @x ok");
        b.make_chip(4..6, "x");
        let l = lay(&b, 200);
        assert_eq!(l.paras[0].chip_runs, vec![false, true, false]);
        let mut r = EditLayout::new(
            typo(),
            FontKey {
                size: 10,
                ..Default::default()
            },
        );
        r.rtl = true;
        r.rebuild("abc", &[], 100, &mut FakeMetrics);
        assert_eq!(r.paras[0].layout.lines[0].x, 100 - 18);
        assert_eq!(r.caret("abc", 3, false, &mut FakeMetrics).x, 100);
    }

    #[test]
    fn selection_rects_cover_lines() {
        let b = EditBuffer::new("hello brave new world");
        let l = lay(&b, 60);
        let r = l.rects(b.text(), 2..18, 0, 1000, &mut FakeMetrics);
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].0, 12);
    }

    #[test]
    fn merged_changes_cover_every_edit() {
        let mut b = EditBuffer::new("one\ntwo\nthree\nfour");
        let mut l = lay(&b, 200);
        b.take_changes();
        b.move_to(15, false);
        b.insert("X");
        b.move_to(1, false);
        b.backspace();
        b.move_to(5, false);
        b.insert("\n\n");
        let ch = b.take_changes();
        l.update(b.text(), b.chips(), &ch, &mut FakeMetrics);
        let full = lay(&b, 200);
        let spans = |l: &EditLayout| l.paras.iter().map(|p| (p.start, p.len)).collect::<Vec<_>>();
        assert_eq!(spans(&l), spans(&full));
    }

    #[test]
    fn long_paragraph_edits_match_a_full_layout() {
        let base =
            "lorem ipsum dolor sit amet, consectetur adipiscing elit. 漢字かな交じり文も折り返す。"
                .repeat(40);
        for (at, del, ins) in [
            (5usize, 0usize, "XYZ "),
            (300, 4, ""),
            (900, 0, "a much longer insertion that adds words "),
            (0, 6, ""),
            (base.len(), 0, " end"),
            (1200, 30, "!"),
        ] {
            let mut b = EditBuffer::new(&base);
            let mut l = lay(&b, 120);
            b.take_changes();
            let at = b.snap(at.min(b.len()), Some(false));
            let end = b.snap((at + del).min(b.len()), Some(true));
            b.replace(at..end, ins, Vec::new(), crate::edit::EditKind::Other);
            let ch = b.take_changes();
            l.update(b.text(), b.chips(), &ch, &mut FakeMetrics);
            let full = lay(&b, 120);
            assert!(l.paras[0].layout.lines.len() > 8);
            assert_eq!(l.paras[0].layout, full.paras[0].layout, "edit at {at}");
            assert_eq!(l.height, full.height);
        }
    }

    #[test]
    fn cached_metrics_match() {
        let mut c = CachedMetrics::new(FakeMetrics);
        let f = FontKey {
            size: 10,
            ..Default::default()
        };
        assert_eq!(c.width(f, "word"), FakeMetrics.width(f, "word"));
        assert_eq!(c.width(f, "word"), FakeMetrics.width(f, "word"));
        assert_eq!(c.entries, 1);
    }
}
