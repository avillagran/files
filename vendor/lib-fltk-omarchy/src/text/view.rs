//! `RichText`: a virtualized, selectable, keyboard-first view of a rich
//! text document, made for chat transcripts that stream.

use super::doc::*;
use super::layout::{self, estimate_of, layout_block, FltkMetrics, Layout, Metrics, Typo};
use super::store::Store;
use super::markdown::{MarkdownOptions, MarkdownStream, StreamUpdate};
use crate::paint;
use crate::state::State;
use crate::theme::{self, Rgb, Theme};
use crate::widgets::glyph;
use fltk::{app, draw, enums::*, prelude::*, widget::Widget};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use unicode_segmentation::UnicodeSegmentation;

#[cfg(feature = "a11y")]
#[path = "access.rs"]
mod access;

const BAR: i32 = 4;

/// A place in the document: block index and byte offset in its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct TextPos {
    pub block: usize,
    pub offset: usize,
}

/// Something reachable with Tab inside the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Target {
    Link { block: usize, run: usize },
    Copy { block: usize },
}

impl Target {
    fn block(self) -> usize {
        match self {
            Target::Link { block, .. } | Target::Copy { block } => block,
        }
    }
}

// ----------------------------------------------------------- prefix sums

/// Fenwick tree of block heights: y of a block and the block at a y in
/// O(log n), appends and tail truncation in O(log n).
#[derive(Debug, Default, Clone)]
struct Heights {
    t: Vec<i64>,
}

impl Heights {
    fn len(&self) -> usize {
        self.t.len()
    }
    fn prefix(&self, i: usize) -> i64 {
        let mut s = 0;
        let mut j = i.min(self.t.len());
        while j > 0 {
            s += self.t[j - 1];
            j &= j - 1;
        }
        s
    }
    fn add(&mut self, i: usize, d: i64) {
        let mut j = i + 1;
        while j <= self.t.len() {
            self.t[j - 1] += d;
            j += j & j.wrapping_neg();
        }
    }
    fn push(&mut self, v: i64) {
        let n = self.t.len();
        let j = n + 1;
        let lb = j & j.wrapping_neg();
        let below = self.prefix(n) - self.prefix(n + 1 - lb);
        self.t.push(v + below);
    }
    fn truncate(&mut self, n: usize) {
        self.t.truncate(n);
    }
    /// The value at `i`.
    fn get(&self, i: usize) -> i64 {
        self.prefix(i + 1) - self.prefix(i)
    }
    /// Index of the block that holds y (the last one past the end).
    fn find(&self, y: i64) -> usize {
        let n = self.t.len();
        let (mut pos, mut rem) = (0usize, y);
        let mut step = if n == 0 {
            0
        } else {
            1usize << (usize::BITS - 1 - n.leading_zeros())
        };
        while step > 0 {
            if pos + step <= n && self.t[pos + step - 1] <= rem {
                pos += step;
                rem -= self.t[pos - 1];
            }
            step >>= 1;
        }
        pos.min(n.saturating_sub(1))
    }
}

// ---------------------------------------------------------------- state

/// Hashes block indices (keys of the hot map) with one multiply.
#[derive(Default, Clone, Copy)]
struct BuildIndexHasher;

#[derive(Default)]
struct IndexHasher(u64);

impl std::hash::Hasher for IndexHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0.rotate_left(5) ^ *b as u64).wrapping_mul(0x517c_c1b7_2722_0a95);
        }
    }
    fn write_usize(&mut self, n: usize) {
        self.0 = (n as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
}

impl std::hash::BuildHasher for BuildIndexHasher {
    type Hasher = IndexHasher;
    fn build_hasher(&self) -> IndexHasher {
        IndexHasher::default()
    }
}

/// A block near the view: materialized from the store, with its layout.
struct Hot {
    block: Block,
    layout: Option<Layout>,
    /// An earlier version of this block and its layout (incremental relayout).
    prev: Option<(Block, Layout)>,
}

impl Hot {
    fn new(block: Block) -> Box<Hot> {
        Box::new(Hot {
            block,
            layout: None,
            prev: None,
        })
    }
}
/// Blocks far from the view drop their layout (their height stays) when
/// more than this many are hot.
const LIVE_MAX: usize = 600;

struct Press {
    x: i32,
    y: i32,
    link: Option<Arc<str>>,
    moved: bool,
}

pub(super) struct Inner {
    /// Every block, packed. A block laid out once keeps its height in
    /// `heights` (flagged `measured`) after its layout is dropped.
    store: Store,
    /// Blocks near the view (laid out, or about to be): at most about
    /// `LIVE_MAX`, keyed by block index.
    hot: HashMap<usize, Box<Hot>, BuildIndexHasher>,
    /// Gap above + height of each block.
    heights: Heights,
    width: i32,
    typo: Typo,
    avg_char: f32,
    measured_avg: bool,
    top: i64,
    motion: crate::scroll::Scroll,
    stick: bool,
    stuck: bool,
    stream: Option<(usize, MarkdownStream)>,
    opts: MarkdownOptions,
    sel: Option<(TextPos, TextPos)>,
    selecting: bool,
    press: Option<Press>,
    pub hover: Option<Target>,
    pub focus: Option<Target>,
    copied: Option<(usize, Instant)>,
    thumb: Option<(i32, i64)>,
    pad: i32,
    surface: Option<ColorRole>,
    laid_out: usize,
    bound: Option<(usize, u64)>,
    view_h: i32,
    frames: u64,
    paint_time: std::time::Duration,
    /// A new message fading in: its first block and the motion.
    reveal: Option<(usize, crate::anim::Tween)>,
}

impl Inner {
    fn new() -> Inner {
        Inner {
            store: Store::default(),
            hot: HashMap::default(),
            heights: Heights::default(),
            width: 0,
            typo: theme::with(Typo::from_theme),
            avg_char: 0.0,
            measured_avg: false,
            top: 0,
            motion: crate::scroll::Scroll::new(),
            stick: false,
            stuck: false,
            stream: None,
            opts: MarkdownOptions::default(),
            sel: None,
            selecting: false,
            press: None,
            hover: None,
            focus: None,
            copied: None,
            thumb: None,
            pad: theme::with(|t| t.spacing.xl),
            surface: None,
            laid_out: 0,
            bound: None,
            view_h: 0,
            frames: 0,
            paint_time: std::time::Duration::ZERO,
            reveal: None,
        }
    }

    fn avg(&self) -> f32 {
        if self.avg_char > 0.0 {
            self.avg_char
        } else {
            self.typo.body as f32 * 0.6
        }
    }

    fn gap(&self, i: usize) -> i32 {
        if i == 0 {
            0
        } else {
            self.typo
                .gap_of(Some(self.store.shape(i - 1)), self.store.shape(i))
        }
    }

    /// The laid out block `i` (blocks near the view).
    fn laid(&self, i: usize) -> Option<(&Block, &Layout)> {
        let h = self.hot.get(&i)?;
        Some((&h.block, h.layout.as_ref()?))
    }

    fn body_h(&self, i: usize) -> i32 {
        if let Some((_, l)) = self.laid(i) {
            return l.height;
        }
        if self.store.measured(i) && i < self.heights.len() {
            return (self.heights.get(i) - self.gap(i) as i64) as i32;
        }
        estimate_of(
            self.store.shape(i),
            self.store.text(i),
            self.width.max(40),
            &self.typo,
            self.avg(),
        )
    }

    fn refresh_h(&mut self, i: usize) -> bool {
        let h = self.gap(i) + self.body_h(i);
        let d = h as i64 - self.heights.get(i);
        if d != 0 {
            self.heights.add(i, d);
        }
        d != 0
    }

    fn content_h(&self) -> i64 {
        self.heights.prefix(self.heights.len()) + 2 * self.pad as i64
    }

    fn max_top(&self) -> i64 {
        (self.content_h() - self.view_h as i64).max(0)
    }

    fn at_bottom(&self) -> bool {
        self.top >= self.max_top() - 1
    }

    fn push_slot(&mut self, block: Block) {
        self.store.push(&block);
        let i = self.store.len() - 1;
        let h = self.gap(i) + self.body_h(i);
        self.heights.push(h as i64);
    }

    /// Replaces blocks from `from` on; unchanged ones keep their layout.
    fn replace_from(&mut self, from: usize, blocks: Vec<Block>) {
        let old_n = self.store.len();
        let from = from.min(old_n);
        let new_n = from + blocks.len();
        // Compare with the old blocks before their bytes are dropped:
        // `Some(measured height)` for an unchanged block.
        let same: Vec<Option<Option<i32>>> = blocks
            .iter()
            .enumerate()
            .map(|(j, b)| {
                let k = from + j;
                let eq = k < old_n
                    && match self.hot.get(&k) {
                        Some(h) => h.block == *b,
                        None => self.store.same(k, b),
                    };
                eq.then(|| {
                    (self.store.measured(k) && self.laid(k).is_none())
                        .then(|| (self.heights.get(k) - self.gap(k) as i64) as i32)
                })
            })
            .collect();
        for k in new_n..old_n {
            self.hot.remove(&k);
        }
        self.store.truncate(from);
        self.heights.truncate(from);
        for (j, b) in blocks.into_iter().enumerate() {
            let k = from + j;
            self.store.push(&b);
            let mut body = None;
            match same[j] {
                Some(m) => {
                    if self.hot.get(&k).is_some_and(|h| h.layout.is_some()) {
                        self.store.set_measured(k, true);
                    } else if let Some(m) = m {
                        self.store.set_measured(k, true);
                        body = Some(m);
                    }
                }
                None => {
                    // A block that grew keeps its old layout for reuse.
                    if let Some(h) = self.hot.remove(&k) {
                        let prev = match h.layout {
                            Some(l) => Some((h.block, l)),
                            None => h.prev,
                        };
                        if prev.is_some() {
                            self.hot.insert(
                                k,
                                Box::new(Hot {
                                    block: b,
                                    layout: None,
                                    prev,
                                }),
                            );
                        }
                    }
                }
            }
            let h = self.gap(k) + body.unwrap_or_else(|| self.body_h(k));
            self.heights.push(h as i64);
        }
        let n = self.store.len();
        let clamp = |p: TextPos| TextPos {
            block: p.block.min(n.saturating_sub(1)),
            offset: p.offset,
        };
        if let Some((a, b)) = self.sel {
            self.sel = if n == 0 {
                None
            } else {
                Some((clamp(a), clamp(b)))
            };
        }
        if self.focus.is_some_and(|t| t.block() >= from) {
            self.focus = None;
        }
        self.hover = None;
    }

    fn apply(&mut self, start: usize, u: StreamUpdate) {
        self.replace_from(start + u.keep, u.blocks);
    }

    fn clear(&mut self) {
        self.reveal = None;
        self.store.clear();
        self.hot.clear();
        self.heights = Heights::default();
        self.stream = None;
        self.sel = None;
        self.focus = None;
        self.hover = None;
        self.top = 0;
        self.motion.jump(0);
        self.stuck = self.stick;
        self.bound = None;
    }

    fn push_markdown(&mut self, more: &str) {
        if more.is_empty() {
            return;
        }
        if self.stream.is_none() {
            let start = self.store.len();
            self.stream = Some((start, MarkdownStream::with_options(self.opts.clone())));
        }
        let Some((start, s)) = self.stream.as_mut() else {
            return;
        };
        let start = *start;
        let u = s.push(more);
        self.apply(start, u);
    }

    /// Re-estimates every block (width, font or theme change).
    fn relayout_all(&mut self) {
        let n = self.store.len();
        let anchor = (n > 0).then(|| {
            let y = (self.top - self.pad as i64).max(0);
            let k = self.heights.find(y);
            (k, y - self.heights.prefix(k))
        });
        self.hot.clear();
        let mut h = Heights::default();
        for i in 0..n {
            self.store.set_measured(i, false);
        }
        for i in 0..n {
            h.push((self.gap(i) + self.body_h(i)) as i64);
        }
        self.heights = h;
        if let Some((k, off)) = anchor {
            self.top = self.heights.prefix(k) + off.min(self.heights.get(k)) + self.pad as i64;
            self.motion.jump(self.top);
        }
    }

    fn ensure_layout(&mut self, i: usize, m: &mut dyn Metrics) -> bool {
        if self.laid(i).is_some() {
            return false;
        }
        let store = &self.store;
        let hot = self
            .hot
            .entry(i)
            .or_insert_with(|| Hot::new(store.block(i)));
        let prev = hot.prev.take();
        let l = layout_block(
            &hot.block,
            self.width,
            &self.typo,
            m,
            prev.as_ref().map(|p| (&p.0, &p.1)),
        );
        hot.layout = Some(l);
        self.store.set_measured(i, true);
        self.laid_out += 1;
        self.refresh_h(i)
    }

    /// Visible block range for the current top.
    fn visible(&self) -> Range<usize> {
        let n = self.store.len();
        if n == 0 {
            return 0..0;
        }
        let y0 = (self.top - self.pad as i64).max(0);
        let a = self.heights.find(y0);
        let b = self.heights.find(y0 + self.view_h as i64);
        a..(b + 1).min(n)
    }

    /// Lays out what is visible (and half a view around), keeping the
    /// first already measured visible block where it was on screen.
    fn ensure_visible(&mut self, m: &mut dyn Metrics) {
        let n = self.store.len();
        if n == 0 {
            self.top = 0;
            return;
        }
        for _ in 0..8 {
            if self.stuck && !self.motion.running() {
                self.top = self.max_top();
            }
            let y0 = self.top - self.pad as i64;
            let over = self.view_h as i64 / 2;
            let first = self.heights.find((y0 - over).max(0));
            let last = self.heights.find((y0 + self.view_h as i64 + over).max(0));
            let vis = self.heights.find(y0.max(0));
            let anchor = (vis..=last)
                .find(|&k| self.laid(k).is_some())
                .map(|k| (k, self.heights.prefix(k) - y0));
            let mut changed = false;
            for k in first..=last {
                changed |= self.ensure_layout(k, m);
            }
            if !changed {
                break;
            }
            if !(self.stuck && !self.motion.running()) {
                if let Some((k, off)) = anchor {
                    let new_top = self.heights.prefix(k) - off + self.pad as i64;
                    let d = new_top - self.top;
                    if d != 0 {
                        self.top = new_top;
                        self.motion.shift(d);
                    }
                }
            }
            let max = self.max_top();
            self.top = self.top.clamp(0, max);
        }
        if self.stuck && !self.motion.running() {
            self.top = self.max_top();
        }
        self.evict();
    }

    /// Drops the layouts of blocks far from the view when there are many.
    fn evict(&mut self) {
        if self.hot.len() <= LIVE_MAX {
            return;
        }
        let vis = self.visible();
        let margin = 64;
        let keep = vis.start.saturating_sub(margin)..vis.end + margin;
        self.hot.retain(|k, _| keep.contains(k));
    }

    /// A message appended after others fades in (o3 row arrival, 360 ms);
    /// streaming into it later costs nothing more.
    fn reveal_from(&mut self, start: usize) {
        self.reveal = (start > 0 && self.store.len() > start && crate::anim::enabled())
            .then(|| (start, crate::anim::Tween::start(crate::anim::ms::ROW_ARRIVE, crate::anim::Ease::OutCubic)));
    }

    fn screen_y(&self, i: usize, wy: i32) -> i32 {
        (wy as i64 + self.pad as i64 + self.heights.prefix(i) - self.top) as i32 + self.gap(i)
    }

    /// Block under window y (clamped), with its screen top.
    fn block_at(&self, wy: i32, ey: i32) -> Option<(usize, i32)> {
        if self.store.len() == 0 {
            return None;
        }
        let y = (ey as i64 - wy as i64 - self.pad as i64 + self.top).max(0);
        let i = self.heights.find(y);
        Some((i, self.screen_y(i, wy)))
    }

    fn pos_at(
        &mut self,
        wx: i32,
        wy: i32,
        ex: i32,
        ey: i32,
        m: &mut dyn Metrics,
    ) -> Option<TextPos> {
        let (i, by) = self.block_at(wy, ey)?;
        self.ensure_layout(i, m);
        let (b, l) = self.laid(i)?;
        // Above the first block or below the last: the ends.
        let rel_y = ey - by;
        if i + 1 == self.store.len() && rel_y >= l.height {
            return Some(TextPos {
                block: i,
                offset: b.text().len(),
            });
        }
        let off = l.hit(b.text(), ex - (wx + self.pad), rel_y, m);
        Some(TextPos {
            block: i,
            offset: off,
        })
    }

    fn target_at(
        &mut self,
        wx: i32,
        wy: i32,
        ex: i32,
        ey: i32,
        m: &mut dyn Metrics,
    ) -> Option<Target> {
        let (i, by) = self.block_at(wy, ey)?;
        self.ensure_layout(i, m);
        let (b, l) = self.laid(i)?;
        let bx = wx + self.pad;
        if b.is_code() {
            let (x, y, w, h) = copy_rect(l, &self.typo, bx, by);
            return (ex >= x && ex < x + w && ey >= y && ey < y + h)
                .then_some(Target::Copy { block: i });
        }
        let (rx, ry) = (ex - bx, ey - by);
        for ln in &l.lines {
            if ry < ln.y || ry >= ln.y + ln.h {
                continue;
            }
            for f in l.frags_of(ln) {
                let fx = ln.x as f32 + f.x;
                if (rx as f32) >= fx && (rx as f32) < fx + f.w {
                    let run = f.run as usize;
                    return b
                        .content
                        .runs()
                        .get(run)
                        .and_then(|r| r.style.link.as_ref())
                        .map(|_| Target::Link { block: i, run });
                }
            }
        }
        None
    }

    fn link_of(&self, t: Target) -> Option<Arc<str>> {
        match t {
            Target::Link { block, run } => match self.hot.get(&block) {
                Some(h) => h.block.content.runs().get(run)?.style.link.clone(),
                None => self.store.link(block, run).map(Arc::from),
            },
            Target::Copy { .. } => None,
        }
    }

    /// Links and copy buttons of the blocks in view, in reading order.
    fn targets(&self) -> Vec<Target> {
        let mut out = Vec::new();
        for i in self.visible() {
            let Some((b, l)) = self.laid(i) else { continue };
            if b.is_code() {
                out.push(Target::Copy { block: i });
                continue;
            }
            for ln in &l.lines {
                for f in l.frags_of(ln) {
                    let run = f.run as usize;
                    if b.content
                        .runs()
                        .get(run)
                        .is_some_and(|r| r.style.link.is_some())
                    {
                        let t = Target::Link { block: i, run };
                        if !out.contains(&t) {
                            out.push(t);
                        }
                    }
                }
            }
        }
        out
    }

    fn sel_range(&self) -> Option<(TextPos, TextPos)> {
        let (a, b) = self.sel?;
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        (a != b).then_some((a, b))
    }

    fn selected_text(&self) -> String {
        let Some((a, b)) = self.sel_range() else {
            return String::new();
        };
        let mut out = String::new();
        for i in a.block..=b.block.min(self.store.len().saturating_sub(1)) {
            let t = self.store.text(i);
            let s = if i == a.block {
                floor_char(t, a.offset)
            } else {
                0
            };
            let e = if i == b.block {
                floor_char(t, b.offset)
            } else {
                t.len()
            };
            if s < e {
                out.push_str(&t[s..e]);
            }
            if i < b.block {
                out.push('\n');
            }
        }
        out
    }

    fn select_all(&mut self) {
        if let Some(last) = self.store.len().checked_sub(1) {
            self.sel = Some((
                TextPos::default(),
                TextPos {
                    block: last,
                    offset: self.store.text(last).len(),
                },
            ));
        }
    }

    fn scroll_by(&mut self, d: i64) -> bool {
        let max = self.max_top();
        let old = self.top;
        self.top = (self.top + d).clamp(0, max);
        self.motion.jump(self.top);
        self.stuck = self.stick && self.top >= max;
        self.top != old
    }
}

fn floor_char(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn copy_rect(l: &Layout, ty: &Typo, bx: i32, by: i32) -> (i32, i32, i32, i32) {
    let s = ty.code_header;
    (bx + l.left + l.box_w - s - 2, by + 1, s, s - 2)
}

// ----------------------------------------------------------------- widget

type LinkFn = Rc<RefCell<Option<Box<dyn FnMut(&str)>>>>;

/// Rich text view: Markdown or [`Block`]s, word-wrapped, virtualized
/// (only the blocks in view are laid out and drawn), selectable, with
/// links, code blocks and tables. Built for chat transcripts: append
/// messages, stream tokens into the last one, and keep the view at the
/// bottom while it grows.
///
/// ```no_run
/// use fltk_omarchy::text::RichText;
/// let chat = RichText::new().stick_to_bottom(true).on_link(|url| println!("open {url}"));
/// chat.append_markdown("**You**\n\nWhat is a monad?");
/// chat.append_markdown("");                      // a new message for the reply…
/// chat.push_markdown("A monad is ");             // …streamed token by token
/// chat.push_markdown("a *monoid* in the category of endofunctors.");
/// ```
///
/// Keys (focused): Up / Down, PageUp / PageDown, Home / End scroll; Tab and
/// Shift+Tab go through the links and code copy buttons in view, then on
/// to the next control; Enter or Space opens the focused link (or copies the
/// code); Ctrl+A selects everything, Ctrl+C copies the selection (Cmd on
/// macOS); Esc clears the selection.
#[derive(Clone)]
pub struct RichText {
    widget: Widget,
    inner: Rc<RefCell<Inner>>,
    link: LinkFn,
}
fltk::widget_extends!(RichText, Widget, widget);

impl Default for RichText {
    fn default() -> Self {
        Self::new()
    }
}

impl RichText {
    pub fn new() -> RichText {
        let mut widget = Widget::default().with_size(320, 240);
        widget.set_frame(FrameType::NoBox);
        widget.set_visible_focus();
        let rt = RichText {
            widget,
            inner: Rc::new(RefCell::new(Inner::new())),
            link: Rc::default(),
        };
        rt.install();
        rt
    }

    fn install(&self) {
        let inn = self.inner.clone();
        let mut w = self.widget.clone();
        w.draw(move |w| {
            let t0 = Instant::now();
            let mut i = inn.borrow_mut();
            theme::with(|t| draw_view(w, &mut i, t));
            i.frames += 1;
            i.paint_time += t0.elapsed();
        });
        let (inn, link) = (self.inner.clone(), self.link.clone());
        w.handle(move |w, ev| handle(w, &inn, &link, ev));
        #[cfg(feature = "a11y")]
        access::install(self);
    }

    // ------------------------------------------------------------ builders

    /// Follow the end while new text arrives, until the user scrolls up;
    /// End or scrolling back to the bottom follows again.
    pub fn stick_to_bottom(self, on: bool) -> Self {
        {
            let mut i = self.inner.borrow_mut();
            i.stick = on;
            i.stuck = on;
        }
        self
    }

    /// Called with the URL of a clicked link (or Enter on a focused one).
    pub fn on_link(self, f: impl FnMut(&str) + 'static) -> Self {
        *self.link.borrow_mut() = Some(Box::new(f));
        self
    }

    /// Markdown conversion options (syntax highlighting).
    pub fn markdown_options(self, opts: MarkdownOptions) -> Self {
        self.inner.borrow_mut().opts = opts;
        self
    }

    /// Padding around the text (default: the theme's `xl` spacing).
    pub fn padding(self, px: i32) -> Self {
        self.inner.borrow_mut().pad = px.max(0);
        self
    }

    /// Background role (default: the window background).
    pub fn surface(self, role: ColorRole) -> Self {
        self.inner.borrow_mut().surface = Some(role);
        self
    }

    /// Shows the Markdown in `s` and follows it: when the new value extends
    /// the old one (streaming into a `State`), only the new text is parsed;
    /// any other change re-parses it all. `State::push_str` / `text_feed`
    /// say so directly (`Edit::Appended`), so the old text is not even
    /// re-hashed; plain `set` compares the prefix. For high-rate streams prefer
    /// [`push_markdown`](Self::push_markdown) or [`sender`](Self::sender).
    pub fn bind_markdown(self, s: &State<String>) -> Self {
        let (inn, mut w) = (self.inner.clone(), self.widget.clone());
        crate::state::bind_widget_edit(&self.widget, s, move |v: &String, edit| {
            let mut i = inn.borrow_mut();
            let (old_len, old_h) = i.bound.unwrap_or((0, FNV0));
            // `push_str` says it only appended: no need to hash the prefix.
            let extends = match edit {
                crate::state::Edit::Appended { from } if i.bound.is_some() && from >= old_len && v.len() >= old_len => true,
                _ => i.bound.is_some_and(|(len, h)| {
                    v.len() >= len && v.is_char_boundary(len) && fnv(FNV0, &v.as_bytes()[..len]) == h
                }),
            };
            if extends && i.stream.is_some() {
                i.push_markdown(&v[old_len..]);
                i.bound = Some((v.len(), fnv(old_h, &v.as_bytes()[old_len..])));
            } else {
                i.clear();
                i.push_markdown(v);
                if i.stream.is_none() {
                    let n = i.store.len();
                    let o = i.opts.clone();
                    i.stream = Some((n, MarkdownStream::with_options(o)));
                }
                i.bound = Some((v.len(), fnv(FNV0, v.as_bytes())));
            }
            drop(i);
            w.redraw();
        });
        self
    }

    // ------------------------------------------------------------- content

    /// Replaces everything with this Markdown.
    pub fn set_markdown(&self, md: &str) {
        self.inner.borrow_mut().clear();
        self.append_markdown(md);
    }

    /// Appends a new section (a chat message) with this Markdown. It stays
    /// open: [`push_markdown`](Self::push_markdown) continues it.
    pub fn append_markdown(&self, md: &str) {
        {
            let mut i = self.inner.borrow_mut();
            let start = i.store.len();
            let o = i.opts.clone();
            i.stream = Some((start, MarkdownStream::with_options(o)));
            i.push_markdown(md);
            i.reveal_from(start);
        }
        self.changed();
    }

    /// Streams more Markdown into the open section (a new one if none):
    /// only its open tail is parsed again and only changed blocks are laid
    /// out again.
    pub fn push_markdown(&self, more: &str) {
        if more.is_empty() {
            return;
        }
        self.inner.borrow_mut().push_markdown(more);
        self.changed();
    }

    /// Closes the open section (frees its parser state).
    pub fn finish(&self) {
        self.inner.borrow_mut().stream = None;
    }

    /// Appends blocks built in code (closes the open section).
    pub fn append_blocks(&self, blocks: impl IntoIterator<Item = Block>) {
        {
            let mut i = self.inner.borrow_mut();
            i.stream = None;
            let start = i.store.len();
            for b in blocks {
                i.push_slot(b);
            }
            i.reveal_from(start);
        }
        self.changed();
    }

    pub fn append_block(&self, b: Block) {
        self.append_blocks([b]);
    }

    pub fn set_document(&self, doc: Document) {
        self.inner.borrow_mut().clear();
        self.append_blocks(doc.blocks);
    }

    pub fn clear(&self) {
        self.inner.borrow_mut().clear();
        self.changed();
    }

    fn changed(&self) {
        self.widget.clone().redraw();
    }

    /// A `Send` handle that streams Markdown from any thread: text pushed
    /// between two frames is joined and parsed once on the UI thread.
    pub fn sender(&self) -> TextSender {
        let id = SENDER_ID.with(|n| {
            let v = n.get() + 1;
            n.set(v);
            v
        });
        // SAFETY: a live widget; the copy only tracks its deletion.
        let guard = unsafe { Widget::from_widget_ptr(self.widget.as_widget_ptr()) };
        SENDERS.with(|s| {
            let mut s = s.borrow_mut();
            s.retain(|(_, w, _)| !w.was_deleted());
            s.push((id, guard, Rc::downgrade(&self.inner)));
        });
        TextSender {
            id,
            buf: Arc::new(Mutex::new(Pending::default())),
        }
    }

    // ------------------------------------------------------------- queries

    pub fn block_count(&self) -> usize {
        self.inner.borrow().store.len()
    }

    pub fn block(&self, i: usize) -> Option<Block> {
        let inner = self.inner.borrow();
        if i >= inner.store.len() {
            return None;
        }
        Some(match inner.hot.get(&i) {
            Some(h) => h.block.clone(),
            None => inner.store.block(i),
        })
    }

    /// The whole text, one block per line (for accessibility and search);
    /// [`block_ranges`](Self::block_ranges) maps blocks into it.
    pub fn plain_text(&self) -> String {
        let i = self.inner.borrow();
        let n = i.store.len();
        let mut out = String::with_capacity((0..n).map(|k| i.store.text(k).len() + 1).sum());
        for k in 0..n {
            if k > 0 {
                out.push('\n');
            }
            out.push_str(i.store.text(k));
        }
        out
    }

    pub fn block_ranges(&self) -> Vec<Range<usize>> {
        let i = self.inner.borrow();
        let mut at = 0;
        (0..i.store.len())
            .map(|k| {
                let r = at..at + i.store.text(k).len();
                at = r.end + 1;
                r
            })
            .collect()
    }

    /// Selection as (start, end), ordered; None when nothing is selected.
    pub fn selection(&self) -> Option<(TextPos, TextPos)> {
        self.inner.borrow().sel_range()
    }

    pub fn set_selection(&self, a: TextPos, b: TextPos) {
        self.inner.borrow_mut().sel = Some((a, b));
        self.changed();
    }

    pub fn select_all(&self) {
        self.inner.borrow_mut().select_all();
        self.changed();
    }

    pub fn selected_text(&self) -> String {
        self.inner.borrow().selected_text()
    }

    /// Copies the selection to the clipboard; false when nothing is selected.
    pub fn copy_selection(&self) -> bool {
        let t = self.selected_text();
        if t.is_empty() {
            return false;
        }
        app::copy(&t);
        true
    }

    /// Scroll offset in pixels.
    pub fn scroll_offset(&self) -> i64 {
        self.inner.borrow().top
    }

    pub fn scroll_to(&self, top: i64) {
        {
            let mut i = self.inner.borrow_mut();
            let max = i.max_top();
            let top = top.clamp(0, max.max(0));
            i.top = top;
            i.motion.jump(top);
            i.stuck = i.stick && i.top >= max;
        }
        self.changed();
    }

    pub fn scroll_to_bottom(&self) {
        {
            let mut i = self.inner.borrow_mut();
            i.stuck = i.stick;
            let top = i.max_top();
            i.top = top;
            i.motion.jump(top);
        }
        self.changed();
    }

    /// True when the view shows the end.
    pub fn is_at_bottom(&self) -> bool {
        self.inner.borrow().at_bottom()
    }

    /// Content height (estimates for blocks never laid out).
    pub fn content_height(&self) -> i64 {
        self.inner.borrow().content_h()
    }

    /// Frames painted and the time spent in them (layout included).
    pub fn paint_stats(&self) -> (u64, std::time::Duration) {
        let i = self.inner.borrow();
        (i.frames, i.paint_time)
    }

    /// Blocks laid out so far (proves virtualization in tests).
    pub fn blocks_laid_out(&self) -> usize {
        self.inner.borrow().laid_out
    }

    /// URL of the link focused with Tab.
    pub fn focused_link(&self) -> Option<String> {
        let i = self.inner.borrow();
        i.focus.and_then(|t| i.link_of(t)).map(|s| s.to_string())
    }

    /// Window rectangle (x, y, w, h) of the first occurrence of `needle`
    /// in the blocks in view, on one line (tests, tours, popups anchored
    /// to a word). Only blocks that have been drawn are searched.
    pub fn text_rect(&self, needle: &str) -> Option<(i32, i32, i32, i32)> {
        let i = self.inner.borrow();
        let mut m = FltkMetrics::default();
        let (wx, wy) = (self.widget.x(), self.widget.y());
        for k in i.visible() {
            let Some((b, l)) = i.laid(k) else { continue };
            let text = b.text();
            let Some(off) = text.find(needle) else {
                continue;
            };
            let Some(li) = l
                .lines
                .iter()
                .position(|ln| (ln.start as usize) <= off && off < ln.end as usize)
            else {
                continue;
            };
            let ln = &l.lines[li];
            let end = (off + needle.len()).min(ln.end as usize);
            // All its pieces in mixed-direction text.
            let (x0, x1) = l.span_x(text, li, off..end, &mut m);
            let by = i.screen_y(k, wy);
            return Some((
                wx + i.pad + x0.round() as i32,
                by + ln.y,
                (x1 - x0).round() as i32,
                ln.h,
            ));
        }
        None
    }
}

// ------------------------------------------------------------ cross-thread

#[derive(Default)]
struct Pending {
    text: String,
    finish: bool,
    posted: bool,
    last: Option<Instant>,
}

/// Streams Markdown into a [`RichText`] from any thread (see
/// [`RichText::sender`]). Text arrives on the UI thread in order, at most
/// 60 times a second: what accumulates in between is applied as one push
/// (one parse of the tail, one layout, one paint).
#[derive(Clone)]
pub struct TextSender {
    id: u64,
    buf: Arc<Mutex<Pending>>,
}

/// Live senders: id, the view's widget (deletion guard), its state.
type SenderEntry = (u64, Widget, Weak<RefCell<Inner>>);

thread_local! {
    static SENDER_ID: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static SENDERS: RefCell<Vec<SenderEntry>> = const { RefCell::new(Vec::new()) };
}

impl TextSender {
    pub fn push(&self, more: &str) {
        self.post(|p| p.text.push_str(more));
    }

    /// Closes the open section after the text pushed so far.
    pub fn finish(&self) {
        self.post(|p| p.finish = true);
    }

    fn post(&self, f: impl FnOnce(&mut Pending)) {
        let mut p = self.buf.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut p);
        if p.posted {
            return;
        }
        p.posted = true;
        drop(p);
        let (id, buf) = (self.id, self.buf.clone());
        crate::task::post(move || schedule(id, buf));
    }
}

/// On the UI thread: delivers now, or at the next 60 Hz frame.
fn schedule(id: u64, buf: Arc<Mutex<Pending>>) {
    const FRAME: f64 = 1.0 / 60.0;
    let last = buf.lock().unwrap_or_else(|e| e.into_inner()).last;
    let wait = last.map_or(0.0, |t| FRAME - t.elapsed().as_secs_f64());
    if wait <= 0.0 {
        deliver(id, &buf);
    } else {
        app::add_timeout3(wait, move |_| deliver(id, &buf));
    }
}

fn deliver(id: u64, buf: &Arc<Mutex<Pending>>) {
    let (text, finish) = {
        let mut p = buf.lock().unwrap_or_else(|e| e.into_inner());
        p.posted = false;
        p.last = Some(Instant::now());
        (
            std::mem::take(&mut p.text),
            std::mem::replace(&mut p.finish, false),
        )
    };
    let target = SENDERS.with(|s| {
        s.borrow()
            .iter()
            .find(|(i, w, _)| *i == id && !w.was_deleted())
            .map(|(_, w, inn)| (w.clone(), inn.clone()))
    });
    let Some((mut w, inn)) = target else { return };
    let Some(inn) = inn.upgrade() else { return };
    {
        let mut i = inn.borrow_mut();
        i.push_markdown(&text);
        if finish {
            i.stream = None;
        }
    }
    w.redraw();
}

const FNV0: u64 = 0xcbf2_9ce4_8422_2325;

fn fnv(mut h: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

// ---------------------------------------------------------------- drawing

fn sel_color(t: &Theme, under: Rgb) -> Rgb {
    t.palette
        .foreground
        .over(under, t.controls.selection_fill_alpha)
}

fn draw_view(w: &mut Widget, i: &mut Inner, t: &Theme) {
    let mut m = FltkMetrics::default();
    let ty = Typo::from_theme(t);
    if ty != i.typo || !i.measured_avg {
        i.typo = ty;
        let sample = "The quick brown fox jumps over the lazy dog";
        i.avg_char = m.width(
            layout::FontKey {
                size: i.typo.body,
                ..Default::default()
            },
            sample,
        ) / sample.chars().count() as f32;
        i.measured_avg = true;
        i.relayout_all();
    }
    let width = (w.w() - 2 * i.pad - BAR - 2).max(40);
    i.view_h = w.h();
    if width != i.width {
        i.width = width;
        i.relayout_all();
    }
    let max = i.max_top();
    let top = i.top;
    i.top = i.motion.position(top, max);
    i.ensure_visible(&mut m);
    if i.motion.running() {
        crate::anim::request_redraw(w);
    }
    let under = i.surface.map_or(t.palette.background, |r| r.rgb(t));
    draw::push_clip(w.x(), w.y(), w.w(), w.h());
    crate::glass::fill(w.x(), w.y(), w.w(), w.h(), 0, under);
    let sel = i.sel_range();
    let ring = crate::focus::show_ring(w);
    let bx = w.x() + i.pad;
    let clip = (w.y(), w.y() + w.h());
    for k in i.visible() {
        let by = i.screen_y(k, w.y());
        let Some((b, l)) = i.laid(k) else { continue };
        let range = sel.and_then(|(a, b)| {
            if k < a.block || k > b.block {
                return None;
            }
            let s0 = if k == a.block { a.offset } else { 0 };
            let e0 = if k == b.block { b.offset } else { usize::MAX };
            Some(s0..e0)
        });
        let prev_quote = if k > 0 { i.store.shape(k - 1).quote } else { 0 };
        let gap = i.gap(k);
        let ctx = BlockCtx {
            t,
            ty: &i.typo,
            under,
            sel: range,
            hover: i.hover.filter(|h| h.block() == k),
            focus: if ring || crate::focus::is_focused(w) {
                i.focus.filter(|h| h.block() == k)
            } else {
                None
            },
            copied: i
                .copied
                .is_some_and(|(b, at)| b == k && at.elapsed().as_secs_f32() < 1.5),
            prev_quote,
            gap,
            clip,
            color: None,
        };
        draw_block(&ctx, b, l, bx, by, &mut m);
    }
    // A new message fading in over what is under it.
    if let Some((start, tw)) = i.reveal {
        if tw.done() || !crate::anim::enabled() || start >= i.store.len() {
            i.reveal = None;
        } else {
            let y0 = i.screen_y(start, w.y()).max(w.y());
            let bottom = w.y() + w.h();
            if y0 < bottom {
                paint::alpha_fill(w.x(), y0, w.w() - BAR - 2, bottom - y0, under, 1.0 - tw.value());
            }
            crate::anim::request_redraw(w);
        }
    }
    // Scrollbar.
    let content = i.content_h();
    if content > w.h() as i64 {
        let track = w.h();
        let thumb = ((w.h() as f64 / content as f64) * track as f64).max(24.0) as i32;
        let maxt = i.max_top().max(1);
        let tyy = w.y() + ((i.top as f64 / maxt as f64) * (track - thumb) as f64) as i32;
        paint::fill(
            w.x() + w.w() - BAR,
            tyy,
            BAR,
            thumb,
            BAR / 2,
            t.palette.foreground.over(under, 0.25),
        );
    }
    draw::pop_clip();
    if ring && i.focus.is_none() {
        let c = t.border(&t.controls.focus, under);
        paint::stroke(
            w.x(),
            w.y(),
            w.w(),
            w.h(),
            t.radius,
            t.controls.focus.border_width.max(1),
            c,
        );
    }
}

pub(super) struct BlockCtx<'a> {
    pub t: &'a Theme,
    pub ty: &'a Typo,
    pub under: Rgb,
    pub sel: Option<Range<usize>>,
    pub hover: Option<Target>,
    pub focus: Option<Target>,
    pub copied: bool,
    pub prev_quote: u8,
    pub gap: i32,
    pub clip: (i32, i32),
    /// Text color for runs without a color role (labels).
    pub color: Option<Rgb>,
}

pub(super) fn draw_block(
    c: &BlockCtx,
    b: &Block,
    l: &Layout,
    bx: i32,
    by: i32,
    m: &mut FltkMetrics,
) {
    let t = c.t;
    let p = &t.palette;
    let ty = c.ty;
    let text = b.text();
    let fg = p.foreground;
    let hair = fg.over(c.under, 0.12);
    // Quote bars (joined to the block above when it is quoted too).
    for q in 0..b.quote {
        // Right-to-left blocks: bars on the right.
        let x = bx + l.mirror_x(q as i32 * ty.quote_w, 3);
        let up = if c.prev_quote > q { c.gap } else { 0 };
        let color = b
            .quote_tone
            .filter(|_| q + 1 == b.quote)
            .map_or(fg.over(c.under, 0.3), |r| r.rgb(t));
        paint::fill(x, by - up, 3, l.height + up, 1, color);
    }
    let surface = p.lighter_background;
    match &b.kind {
        BlockKind::Code { lang } => {
            let (x, w) = (bx + l.left, l.box_w);
            crate::glass::fill(x, by, w, l.height, t.radius.min(8), surface);
            paint::stroke(x, by, w, l.height, t.radius.min(8), 1, hair);
            if !lang.is_empty() {
                paint::text(
                    lang,
                    x + ty.code_pad,
                    by,
                    w / 2,
                    ty.code_header,
                    ty.small,
                    false,
                    fg.darker(1.4),
                    Align::Left,
                );
            }
            let (cx, cy, cw, ch) = copy_rect(l, ty, bx, by);
            if c.hover.is_some() || c.focus.is_some() {
                paint::fill(
                    cx,
                    cy,
                    cw,
                    ch,
                    t.radius.min(4),
                    t.fill(&t.controls.hover, surface),
                );
            }
            let g = if c.copied { glyph::CHECK } else { COPY };
            paint::glyph(
                g,
                cx,
                cy,
                cw,
                ch,
                ty.small + 2,
                if c.copied { p.green } else { fg.darker(1.4) },
            );
            if c.focus.is_some() {
                paint::stroke(cx, cy, cw, ch, t.radius.min(4), 1, p.accent);
            }
        }
        BlockKind::Rule => {
            paint::fill(
                bx + l.left,
                by + l.height / 2,
                l.box_w,
                1,
                0,
                fg.over(c.under, 0.2),
            );
        }
        BlockKind::Table(_) => {
            if let Some(g) = &l.table {
                let x0 = bx + l.left;
                if g.header_rows > 0 {
                    let hy = g.rows.get(g.header_rows - 1).map_or(0, |(y, h)| y + h);
                    crate::glass::fill(x0, by, l.box_w, hy, 0, surface);
                }
                for (ri, (ry, _)) in g.rows.iter().enumerate().skip(1) {
                    let _ = ri;
                    paint::fill(x0, by + ry, l.box_w, 1, 0, hair);
                }
                // Every column edge but the table's left one (columns run
                // right to left in a right-to-left table).
                for (cx, _) in g.cols.iter().filter(|(cx, _)| *cx > l.left) {
                    paint::fill(bx + cx, by, 1, l.height - 1, 0, hair);
                }
                paint::stroke(x0, by, l.box_w, l.height, 0, 1, hair);
            }
        }
        BlockKind::Heading(lv) if *lv <= 2 => {
            paint::fill(
                bx + l.mirror_x(l.left, l.box_w),
                by + l.height - ty.item_gap - 1,
                l.box_w,
                1,
                0,
                hair,
            );
        }
        _ => {}
    }
    // List marker on the first line.
    if let (Some(mk), Some(first)) = (b.marker, l.lines.first()) {
        let base = by + first.y + first.base;
        let size = ty.body;
        let mx = bx + l.left - ty.list_w;
        let dim = fg.darker(1.4);
        layout::FltkMetrics::set(layout::FontKey {
            size,
            ..Default::default()
        });
        match mk {
            ListMarker::Bullet => {
                let d = (size / 3).max(4);
                let cx = bx + l.mirror_x(mx - bx + ty.list_w / 2 - d / 2, d);
                let cy = by + first.y + first.h / 2 - d / 2;
                draw::set_draw_color(dim.fl());
                match b.indent {
                    0 | 1 => draw::draw_pie(cx, cy, d, d, 0.0, 360.0),
                    2 => draw::draw_arc(cx, cy, d, d, 0.0, 360.0),
                    _ => draw::draw_rectf(cx, cy, d, d),
                }
            }
            ListMarker::Ordered(n) => {
                let s = format!("{n}.");
                let wn = m.width(
                    layout::FontKey {
                        size,
                        ..Default::default()
                    },
                    &s,
                );
                draw::set_draw_color(dim.fl());
                let wn = wn.ceil() as i32;
                draw::draw_text_n(&s, bx + l.mirror_x(l.left - 6 - wn, wn), base);
            }
            ListMarker::Task(done) => {
                let d = (size as f32 * 1.05).round() as i32;
                let cx = bx + l.mirror_x(mx - bx + ty.list_w / 2 - d / 2, d);
                let cy = by + first.y + (first.h - d) / 2;
                if done {
                    paint::fill(cx, cy, d, d, 3, p.accent);
                    paint::glyph(glyph::CHECKMARK, cx, cy, d, d, size, p.background);
                } else {
                    paint::stroke(cx, cy, d, d, 3, 1, dim);
                }
            }
        }
    }
    // Selection under the text.
    if let Some(r) = &c.sel {
        let r = r.start.min(text.len())..r.end.min(text.len() + 1);
        let sc = sel_color(t, c.under);
        for (x, y, w, h) in l.rects(text, r, m) {
            paint::fill(bx + x, by + y, w, h, 0, sc);
        }
    }
    // Text.
    let base_color = match &b.kind {
        _ if c.color.is_some() => c.color.unwrap_or(fg),
        BlockKind::Heading(_) => p.bright_foreground,
        _ if b.quote > 0 => fg.darker(1.25),
        _ => fg,
    };
    let runs = b.content.runs();
    let first_line = if l.table.is_some() {
        0
    } else {
        l.lines.partition_point(|ln| by + ln.y + ln.h < c.clip.0)
    };
    for ln in &l.lines[first_line..] {
        let ly = by + ln.y;
        if ly > c.clip.1 {
            if l.table.is_some() {
                continue;
            }
            break;
        }
        if ly + ln.h < c.clip.0 {
            continue;
        }
        let base_y = ly + ln.base;
        for f in l.frags_of(ln) {
            if f.tab {
                continue;
            }
            let Some(r) = runs.get(f.run as usize) else {
                continue;
            };
            let st = &r.style;
            let x = bx + ln.x + f.x.round() as i32;
            let fw = f.w.ceil() as i32;
            if st.code && !b.is_code() {
                paint::fill(x - 2, ly + 1, fw + 4, ln.h - 2, 3, surface);
            }
            let link = st.link.is_some();
            let color = match st.color {
                Some(role) => role.rgb(t),
                None if link => p.accent,
                None => base_color,
            };
            FltkMetrics::set(f.font);
            draw::set_draw_color(color.fl());
            draw::draw_text_n(&text[f.start as usize..f.end as usize], x, base_y);
            if link || st.strike {
                draw::set_draw_color(color.fl());
                if link {
                    draw::draw_rectf(x, base_y + 2, fw, 1);
                }
                if st.strike {
                    draw::draw_rectf(x, base_y - (f.font.size * 3 / 10).max(2), fw, 1);
                }
            }
            let target = Target::Link {
                block: 0,
                run: f.run as usize,
            };
            let is = |o: Option<Target>| {
                o.is_some_and(|o| matches!((o, target), (Target::Link { run: a, .. }, Target::Link { run: b, .. }) if a == b))
            };
            if link && is(c.focus) {
                paint::stroke(x - 2, ly, fw + 4, ln.h, 3, 1, p.accent);
            } else if link && is(c.hover) {
                draw::set_draw_color(color.fl());
                draw::draw_rectf(x, base_y + 3, fw, 1);
            }
        }
    }
}

/// Nerd Font "content copy".
const COPY: &str = "\u{f018f}";

// --------------------------------------------------------------- events

fn handle(w: &mut Widget, inn: &Rc<RefCell<Inner>>, link: &LinkFn, ev: Event) -> bool {
    let mut m = FltkMetrics::default();
    let (wx, wy) = (w.x(), w.y());
    match ev {
        Event::Focus => {
            inn.borrow_mut().focus = None;
            w.redraw();
            true
        }
        Event::Unfocus => {
            inn.borrow_mut().focus = None;
            w.redraw();
            true
        }
        Event::Enter => true,
        Event::Move => {
            let (ex, ey) = (app::event_x(), app::event_y());
            let mut i = inn.borrow_mut();
            let over_bar = ex >= wx + w.w() - BAR - 4;
            let h = if over_bar {
                None
            } else {
                i.target_at(wx, wy, ex, ey, &mut m)
            };
            if h != i.hover {
                i.hover = h;
                drop(i);
                set_cursor(
                    w,
                    if h.is_some() {
                        Cursor::Hand
                    } else {
                        Cursor::Default
                    },
                );
                w.redraw();
            }
            true
        }
        Event::Leave => {
            let had = inn.borrow_mut().hover.take().is_some();
            set_cursor(w, Cursor::Default);
            if had {
                w.redraw();
            }
            true
        }
        Event::Push => {
            let _ = w.take_focus();
            let (ex, ey) = (app::event_x(), app::event_y());
            let mut i = inn.borrow_mut();
            let top = i.top;
            i.motion.jump(top);
            if ex >= wx + w.w() - BAR - 4 {
                i.thumb = Some((ey, top));
                return true;
            }
            if let Some(Target::Copy { block }) = i.target_at(wx, wy, ex, ey, &mut m) {
                let text = i.store.text(block).to_string();
                i.copied = Some((block, Instant::now()));
                drop(i);
                app::copy(&text);
                let mut ww = w.clone();
                app::add_timeout3(1.6, move |_| {
                    if !ww.was_deleted() {
                        ww.redraw()
                    }
                });
                w.redraw();
                return true;
            }
            let target = i.target_at(wx, wy, ex, ey, &mut m);
            let link = target.and_then(|t| i.link_of(t));
            let Some(pos) = i.pos_at(wx, wy, ex, ey, &mut m) else {
                return true;
            };
            let clicks = app::event_clicks_num();
            let shift = app::event_state().contains(Shortcut::Shift);
            if shift && i.sel.is_some() {
                if let Some((a, _)) = i.sel {
                    i.sel = Some((a, pos));
                }
            } else if clicks == 1 {
                let t = i.store.text(pos.block);
                let (a, b) = word_at(t, pos.offset);
                i.sel = Some((
                    TextPos {
                        block: pos.block,
                        offset: a,
                    },
                    TextPos {
                        block: pos.block,
                        offset: b,
                    },
                ));
            } else if clicks >= 2 {
                let n = i.store.text(pos.block).len();
                i.sel = Some((
                    TextPos {
                        block: pos.block,
                        offset: 0,
                    },
                    TextPos {
                        block: pos.block,
                        offset: n,
                    },
                ));
            } else {
                i.sel = Some((pos, pos));
            }
            i.focus = None;
            i.selecting = true;
            i.press = Some(Press {
                x: ex,
                y: ey,
                link,
                moved: false,
            });
            drop(i);
            w.redraw();
            true
        }
        Event::Drag => {
            let (ex, ey) = (app::event_x(), app::event_y());
            let mut i = inn.borrow_mut();
            if let Some((y0, top0)) = i.thumb {
                let content = i.content_h() as f64;
                let max = i.max_top();
                i.top = (top0 + ((ey - y0) as f64 * content / w.h().max(1) as f64) as i64)
                    .clamp(0, max);
                let top = i.top;
                i.motion.jump(top);
                i.stuck = i.stick && top >= max;
                drop(i);
                w.redraw();
                return true;
            }
            if !i.selecting {
                return true;
            }
            if let Some(p) = i.press.as_mut() {
                if (ex - p.x).abs() > 3 || (ey - p.y).abs() > 3 {
                    p.moved = true;
                    p.link = None;
                }
            }
            // Drag past an edge scrolls.
            if ey < wy {
                i.scroll_by((ey - wy) as i64);
            } else if ey > wy + w.h() {
                i.scroll_by((ey - wy - w.h()) as i64);
            }
            let cy = ey.clamp(wy, wy + w.h() - 1);
            if let Some(pos) = i.pos_at(wx, wy, ex, cy, &mut m) {
                if let Some((a, _)) = i.sel {
                    let moved = i.press.as_ref().is_some_and(|p| p.moved);
                    if moved {
                        i.sel = Some((a, pos));
                    }
                }
            }
            drop(i);
            w.redraw();
            true
        }
        Event::Released => {
            let mut i = inn.borrow_mut();
            i.thumb = None;
            i.selecting = false;
            let press = i.press.take();
            if i.sel_range().is_none() {
                i.sel = None;
            } else {
                let t = i.selected_text();
                if !t.is_empty() {
                    app::copy2(&t);
                }
            }
            drop(i);
            if let Some(url) = press.filter(|p| !p.moved).and_then(|p| p.link) {
                if let Some(f) = link.borrow_mut().as_mut() {
                    f(&url);
                }
            }
            w.redraw();
            true
        }
        Event::MouseWheel => {
            let mut i = inn.borrow_mut();
            let max = i.max_top();
            let line = i.typo.line_h(i.typo.body) as f64;
            let delta = app::event_dy_value() as f64 * 3.0 * line;
            let top = i.top;
            i.top = i.motion.wheel(top, delta, max);
            if delta < 0.0 {
                i.stuck = false;
            } else if i.stick && top as f64 + delta >= max as f64 - 1.0 {
                i.stuck = true;
            }
            drop(i);
            w.redraw();
            true
        }
        Event::KeyDown => key(w, inn, link),
        _ => false,
    }
}

fn set_cursor(w: &Widget, c: Cursor) {
    if let Some(mut win) = w.window() {
        win.set_cursor(c);
    }
}

/// Word (or run of blanks / one punctuation mark) around `at`.
fn word_at(t: &str, at: usize) -> (usize, usize) {
    let at = floor_char(t, at);
    for (i, w) in t.split_word_bound_indices() {
        if at >= i && at < i + w.len() {
            return (i, i + w.len());
        }
    }
    (at, at)
}

fn key(w: &mut Widget, inn: &Rc<RefCell<Inner>>, link: &LinkFn) -> bool {
    let k = app::event_key();
    let state = app::event_state();
    let command = app::is_event_command();
    let alt = state.contains(Shortcut::Alt);
    let shift = state.contains(Shortcut::Shift);
    let mut i = inn.borrow_mut();
    {
        let top = i.top;
        i.motion.jump(top);
    }
    if command && !alt {
        if k == Key::from_char('c') {
            let text = match i.focus {
                Some(Target::Copy { block }) if i.sel_range().is_none() => {
                    i.store.text(block).to_string()
                }
                _ => i.selected_text(),
            };
            if text.is_empty() {
                return false;
            }
            drop(i);
            app::copy(&text);
            return true;
        }
        if k == Key::from_char('a') {
            i.select_all();
            drop(i);
            w.redraw();
            return true;
        }
        if k == Key::Home || k == Key::End {
            // Ctrl+Home / Ctrl+End as Home / End.
        } else {
            return false;
        }
    }
    let line = i.typo.line_h(i.typo.body) as i64;
    let page = (w.h() as i64 - line).max(line);
    let used = match k {
        Key::Up => i.scroll_by(-line),
        Key::Down => i.scroll_by(line),
        Key::PageUp => i.scroll_by(-page),
        Key::PageDown => i.scroll_by(page),
        Key::Home => {
            let d = -i.top;
            i.scroll_by(d);
            true
        }
        Key::End => {
            let max = i.max_top();
            i.top = max;
            i.motion.jump(max);
            i.stuck = i.stick;
            true
        }
        Key::Tab if !alt && !command => {
            let targets = i.targets();
            let cur = i.focus.and_then(|f| targets.iter().position(|t| *t == f));
            let next = match (cur, shift) {
                (None, false) => targets.first().copied(),
                (None, true) => {
                    // Coming back into the view from below with Shift+Tab
                    // lands on the view itself; one more goes to the last link.
                    targets.last().copied()
                }
                (Some(c), false) => targets.get(c + 1).copied(),
                (Some(c), true) => c.checked_sub(1).and_then(|c| targets.get(c).copied()),
            };
            i.focus = next;
            if let Some(t) = next {
                reveal(&mut i, t, w);
            }
            let used = next.is_some();
            drop(i);
            w.redraw();
            return used;
        }
        Key::Enter | Key::KPEnter => {
            let Some(t) = i.focus else { return false };
            match t {
                Target::Copy { block } => {
                    let text = i.store.text(block).to_string();
                    i.copied = Some((block, Instant::now()));
                    drop(i);
                    app::copy(&text);
                    let mut ww = w.clone();
                    app::add_timeout3(1.6, move |_| {
                        if !ww.was_deleted() {
                            ww.redraw()
                        }
                    });
                }
                Target::Link { .. } => {
                    let url = i.link_of(t);
                    drop(i);
                    if let (Some(url), Some(f)) = (url, link.borrow_mut().as_mut()) {
                        f(&url);
                    }
                }
            }
            w.redraw();
            return true;
        }
        Key::Escape => {
            let had = i.sel.take().is_some() | i.focus.take().is_some();
            drop(i);
            if had {
                w.redraw();
            }
            return had;
        }
        _ if k == Key::from_char(' ') && i.focus.is_some() => {
            drop(i);
            // Space activates like Enter.
            return activate_focus(w, inn, link);
        }
        _ => return false,
    };
    drop(i);
    if used {
        w.redraw();
    }
    used
}

fn activate_focus(w: &mut Widget, inn: &Rc<RefCell<Inner>>, link: &LinkFn) -> bool {
    let (t, url, code) = {
        let i = inn.borrow();
        let Some(t) = i.focus else { return false };
        (
            t,
            i.link_of(t),
            matches!(t, Target::Copy { .. }).then(|| i.store.text(t.block()).to_string()),
        )
    };
    if let Some(code) = code {
        app::copy(&code);
        inn.borrow_mut().copied = Some((t.block(), Instant::now()));
        let mut ww = w.clone();
        app::add_timeout3(1.6, move |_| {
            if !ww.was_deleted() {
                ww.redraw()
            }
        });
    } else if let (Some(url), Some(f)) = (url, link.borrow_mut().as_mut()) {
        f(&url);
    }
    w.redraw();
    true
}

/// Scrolls so the target's block line is fully in view.
fn reveal(i: &mut Inner, t: Target, w: &Widget) {
    let k = t.block();
    let Some((_, l)) = i.laid(k) else {
        return;
    };
    let (y, h) = match t {
        Target::Copy { .. } => (0, i.typo.code_header),
        Target::Link { run, .. } => l
            .lines
            .iter()
            .find(|ln| l.frags_of(ln).iter().any(|f| f.run as usize == run))
            .map_or((0, l.height), |ln| (ln.y, ln.h)),
    };
    let top_y = i.heights.prefix(k) + i.gap(k) as i64 + y as i64 + i.pad as i64;
    let view = w.h() as i64;
    if top_y < i.top {
        i.top = top_y - i.pad as i64;
    } else if top_y + h as i64 > i.top + view {
        i.top = top_y + h as i64 - view + i.pad as i64;
    }
    let max = i.max_top();
    i.top = i.top.clamp(0, max);
    let top = i.top;
    i.motion.jump(top);
    i.stuck = i.stick && top >= max;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heights_prefix_find_push_truncate() {
        let mut h = Heights::default();
        let v = [10i64, 0, 5, 20, 7, 3, 9, 1, 4];
        for x in v {
            h.push(x);
        }
        for i in 0..=v.len() {
            assert_eq!(h.prefix(i), v[..i].iter().sum::<i64>());
        }
        assert_eq!(h.find(0), 0);
        assert_eq!(h.find(9), 0);
        assert_eq!(h.find(10), 2);
        assert_eq!(h.find(15), 3);
        assert_eq!(h.find(1000), v.len() - 1);
        h.add(3, -15);
        assert_eq!(h.prefix(4), 20);
        h.truncate(5);
        h.push(100);
        assert_eq!(h.prefix(6), 20 + 7 + 100);
    }

    #[test]
    fn word_bounds() {
        assert_eq!(word_at("hello brave world", 8), (6, 11));
        assert_eq!(word_at("añob", 2), (0, 5));
    }
}
