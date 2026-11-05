//! [`TextArea`]: the multi-line editor.

use super::buffer::{Chip, EditBuffer};
use super::layout::{CachedMetrics, EditLayout};
use crate::a11y::{self, Action, Node, Request, Role};
use crate::paint;
use crate::platform::{Os, OS};
use crate::state::State;
use crate::text::layout::{FltkMetrics, FontKey, Typo};
use crate::theme;
use fltk::{app, draw, enums::*, prelude::*, widget::Widget};
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

/// What Tab does in a [`TextArea`] (keyboard rule: Tab moves focus).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TabKey {
    /// Tab and Shift+Tab go to the next / previous control (default).
    #[default]
    Leave,
    /// Ctrl+Tab inserts a tab character; Tab still moves focus. Ctrl+Tab
    /// then no longer reaches a `TabBar` while the area has focus.
    CtrlTabInserts,
    /// Tab inserts a tab character (code editors); Shift+Tab and Ctrl+Tab
    /// still move focus out.
    Inserts,
}

const SCROLLBAR: i32 = 6;

/// A callback slot taken out while it runs, so it may call back into the widget.
type HookFn<A, R> = Box<dyn FnMut(A) -> R>;

pub(crate) struct Hook<A, R = ()>(RefCell<Option<HookFn<A, R>>>);

impl<A, R> Default for Hook<A, R> {
    fn default() -> Self {
        Hook(RefCell::new(None))
    }
}

impl<A, R> Hook<A, R> {
    pub(crate) fn set(&self, f: impl FnMut(A) -> R + 'static) {
        *self.0.borrow_mut() = Some(Box::new(f));
    }

    pub(crate) fn is_set(&self) -> bool {
        self.0.borrow().is_some()
    }

    pub(crate) fn call(&self, a: A) -> Option<R> {
        let f = self.0.borrow_mut().take();
        f.map(|mut f| {
            let r = f(a);
            let mut slot = self.0.borrow_mut();
            if slot.is_none() {
                *slot = Some(f);
            }
            r
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Drag {
    Chars,
    Words(usize, usize),
    Paras(usize, usize),
    Bar(i32),
}

pub(crate) struct Inner {
    pub(crate) buf: EditBuffer,
    lay: EditLayout,
    m: CachedMetrics,
    lay_key: (i32, i32, bool, u64),
    scroll: i32,
    upstream: bool,
    goal_x: Option<i32>,
    hover: bool,
    drag: Option<Drag>,
    pub(crate) placeholder: String,
    lines: (usize, usize),
    tab: TabKey,
    read_only: bool,
    dnd: bool,
}

impl Inner {
    fn base_font() -> FontKey {
        theme::with(|t| FontKey {
            size: t.font.body,
            ..Default::default()
        })
    }

    fn typo() -> Typo {
        theme::with(|t| {
            let mut ty = Typo::from_theme(t);
            ty.line = 1.4;
            ty
        })
    }

    /// Lays out again when the width, font, direction or theme changed;
    /// otherwise applies pending edits.
    fn sync(&mut self, text_w: i32) {
        let base = Self::base_font();
        // Not laid out yet (a flex child before its window's first layout):
        // no wrapping rather than one char per line.
        let text_w = if text_w < 24 { 1 << 20 } else { text_w };
        let key = (
            text_w,
            base.size,
            crate::i18n::is_rtl(),
            theme::generation(),
        );
        let changes = self.buf.take_changes();
        if key != self.lay_key || self.lay.paras.is_empty() {
            if key.1 != self.lay_key.1 || key.3 != self.lay_key.3 {
                self.m.clear();
            }
            self.lay_key = key;
            self.lay.typo = Self::typo();
            self.lay.base = base;
            self.lay.rtl = key.2;
            self.lay
                .rebuild(self.buf.text(), self.buf.chips(), key.0, &mut self.m);
        } else if !changes.is_empty() {
            self.lay
                .update(self.buf.text(), self.buf.chips(), &changes, &mut self.m);
        }
    }

    fn caret(&mut self) -> super::layout::CaretRect {
        let (pos, up) = (self.buf.caret(), self.upstream);
        self.lay.caret(self.buf.text(), pos, up, &mut self.m)
    }

    /// Keeps the caret line in view.
    fn reveal(&mut self, view_h: i32) {
        let c = self.caret();
        if c.y < self.scroll {
            self.scroll = c.y;
        } else if c.y + c.h > self.scroll + view_h {
            self.scroll = c.y + c.h - view_h;
        }
        self.clamp_scroll(view_h);
    }

    fn clamp_scroll(&mut self, view_h: i32) {
        self.scroll = self.scroll.clamp(0, (self.lay.height - view_h).max(0));
    }

    /// Height of the text part for the current content.
    fn content_h(&self) -> i32 {
        let lh = self.lay.line_h.max(1);
        let (min, max) = self.lines;
        let lo = lh * min.max(1) as i32;
        let hi = if max == 0 {
            i32::MAX
        } else {
            lh * max.max(min.max(1)) as i32
        };
        self.lay.height.clamp(lo, hi.max(lo))
    }
}

/// Chrome metrics: (pad x, pad y).
fn pads() -> (i32, i32) {
    theme::with(|t| (t.spacing.control_padding_x, t.spacing.input_padding_y))
}

/// The text box of a widget rect: (x, y, w, h).
fn text_box(x: i32, y: i32, w: i32, h: i32) -> (i32, i32, i32, i32) {
    let (px, py) = pads();
    (
        x + px + 1,
        y + py + 1,
        (w - 2 * px - 2).max(8),
        (h - 2 * py - 2).max(1),
    )
}

pub(crate) type DescribeFn = Rc<dyn Fn(&mut Node)>;

#[derive(Default)]
pub(crate) struct Callbacks {
    pub(crate) change: Hook<String>,
    /// App key hook (`on_key`).
    pub(crate) key: Hook<(Key, Shortcut), bool>,
    /// Composer key hook (before the app's).
    pub(crate) pre_key: Hook<(Key, Shortcut), bool>,
    /// Composer: pasted or dropped text (`true` = dropped); true = used.
    pub(crate) paste: Hook<(String, bool), bool>,
    /// The natural height changed (Composer lays itself out).
    pub(crate) height: Hook<i32>,
    /// Caret or text changed (Composer completions).
    pub(crate) caret: Hook<()>,
    pub(crate) focus: Hook<bool>,
    pub(crate) a11y_extra: RefCell<Option<DescribeFn>>,
    pub(crate) a11y_action: Hook<Request, bool>,
}

/// Omarchy multi-line text editor: `TextField` chrome, word wrap,
/// auto-grow from `min` to `max` lines (then it scrolls), placeholder,
/// two-way binding, mouse and keyboard selection, undo/redo with typing
/// groups, input methods, grapheme-correct editing and atomic chips.
///
/// ```no_run
/// use fltk_omarchy::{edit::TextArea, layout::FlexExt, State};
/// # let mut col = fltk_omarchy::layout::column();
/// let notes = State::new(String::new());
/// col.fit(&*TextArea::new().placeholder("Notes").lines(3, 12).bind(&notes));
/// ```
///
/// Keys: arrows, Home/End (line), Mod+Home/End (text), word jumps with
/// Ctrl+Left/Right (Alt on macOS; Cmd+Left/Right go to the line ends
/// there), PageUp/PageDown, Shift extends; Backspace/Delete (Ctrl/Alt for
/// words), Mod+A/C/X/V, Mod+Z undo, Mod+Shift+Z or Mod+Y redo, Enter a
/// new line, Esc clears the selection; Tab leaves (see [`TabKey`]).
#[derive(Clone)]
pub struct TextArea {
    inner: Widget,
    pub(crate) st: Rc<RefCell<Inner>>,
    pub(crate) cbs: Rc<Callbacks>,
}
fltk::widget_extends!(TextArea, Widget, inner);

impl Default for TextArea {
    fn default() -> Self {
        Self::new()
    }
}

impl TextArea {
    pub fn new() -> Self {
        let mut inner = Widget::default();
        inner.set_frame(FrameType::NoBox);
        inner.set_visible_focus();
        let st = Rc::new(RefCell::new(Inner {
            buf: EditBuffer::new(""),
            lay: EditLayout::new(Inner::typo(), Inner::base_font()),
            m: CachedMetrics::new(FltkMetrics::default()),
            lay_key: (0, 0, false, u64::MAX),
            scroll: 0,
            upstream: false,
            goal_x: None,
            hover: false,
            drag: None,
            placeholder: String::new(),
            lines: (3, 12),
            tab: TabKey::Leave,
            read_only: false,
            dnd: false,
        }));
        let cbs = Rc::new(Callbacks::default());
        let me = TextArea {
            inner: inner.clone(),
            st,
            cbs,
        };
        let h = me.natural_h();
        me.inner.clone().set_size(300, h);
        let d = me.clone();
        inner.draw(move |w| d.draw(w));
        let hd = me.clone();
        inner.handle(move |_, ev| hd.handle(ev));
        let th = me.clone();
        theme::on_change_while(&me.inner, move |_| th.after(false, false));
        let ad = me.clone();
        a11y::describe(&me.inner, move |n| ad.describe(n));
        let aa = me.clone();
        a11y::on_action(&me.inner, move |r| aa.act(r));
        me
    }

    // ------------------------------------------------------------ builder

    pub fn placeholder(self, text: &str) -> Self {
        self.st.borrow_mut().placeholder = text.into();
        self.inner.clone().redraw();
        self
    }

    /// Keeps the placeholder in sync with a State (`&i18n::text("Notes")`).
    pub fn bind_placeholder(self, s: &State<String>) -> Self {
        let (st, mut w) = (self.st.clone(), self.inner.clone());
        crate::state::bind_widget(&self.inner, s, true, move |v| {
            st.borrow_mut().placeholder = v.clone();
            w.redraw();
        });
        self
    }

    /// Grows from `min` to `max` lines, then scrolls (`max` 0: no limit).
    /// `lines(n, n)` keeps a fixed height.
    pub fn lines(self, min: usize, max: usize) -> Self {
        self.st.borrow_mut().lines = (min.max(1), max);
        self.after(false, false);
        self
    }

    pub fn tab_key(self, t: TabKey) -> Self {
        self.st.borrow_mut().tab = t;
        self
    }

    /// Text can be selected and copied, not changed.
    pub fn read_only(self, on: bool) -> Self {
        self.st.borrow_mut().read_only = on;
        self
    }

    pub fn on_change(self, f: impl FnMut(String) + 'static) -> Self {
        let prev = self.cbs.change.0.borrow_mut().take();
        let mut prev = prev;
        let mut f = f;
        self.cbs.change.set(move |t: String| {
            if let Some(p) = prev.as_mut() {
                p(t.clone());
            }
            f(t);
        });
        self
    }

    /// Sees every key first: return true to use it (history on Up at the
    /// first line, app keys). Modifiers are masked to Ctrl/Shift/Alt/Meta.
    pub fn on_key(self, f: impl FnMut(Key, Shortcut) -> bool + 'static) -> Self {
        let mut f = f;
        self.cbs.key.set(move |(k, m)| f(k, m));
        self
    }

    /// Two-way binding: edits write the State, State changes replace the
    /// text (one undo step; the caret goes to the end).
    pub fn bind(self, s: &State<String>) -> Self {
        let me = self.clone();
        crate::state::bind_widget(&self.inner, s, true, move |v| {
            if me.st.borrow().buf.text() != v.as_str() {
                me.edit(|b| b.set_text(v));
            }
        });
        let s2 = s.clone();
        self.on_change(move |t| s2.set(t))
    }

    // ------------------------------------------------------------ queries

    pub fn value(&self) -> String {
        self.st.borrow().buf.text().to_string()
    }

    /// Byte offset of the caret.
    pub fn caret(&self) -> usize {
        self.st.borrow().buf.caret()
    }

    /// The selected bytes (empty at the caret).
    pub fn selection(&self) -> Range<usize> {
        self.st.borrow().buf.selection()
    }

    pub fn selected_text(&self) -> String {
        self.st.borrow().buf.selected_text().to_string()
    }

    pub fn chips(&self) -> Vec<Chip> {
        self.st.borrow().buf.chips().to_vec()
    }

    /// The text with chips replaced by `f(chip_text, data)`.
    pub fn expanded_text(&self, f: impl FnMut(&str, &str) -> String) -> String {
        self.st.borrow().buf.expanded(f)
    }

    /// Read access to the buffer (caret, chips, preedit, history).
    pub fn with_buffer<R>(&self, f: impl FnOnce(&EditBuffer) -> R) -> R {
        f(&self.st.borrow().buf)
    }

    /// True while an input method composes (marked text shown).
    pub fn is_composing(&self) -> bool {
        self.st.borrow().buf.preedit().is_some()
    }

    /// Visual lines of the current text at the current width.
    pub fn line_count(&self) -> usize {
        self.sync_now();
        self.st.borrow().lay.line_count()
    }

    /// The caret is on the first visual line (Up would leave the text).
    pub fn caret_on_first_line(&self) -> bool {
        self.sync_now();
        let mut s = self.st.borrow_mut();
        let c = s.caret();
        c.para == 0 && c.line == 0
    }

    /// The caret is on the last visual line.
    pub fn caret_on_last_line(&self) -> bool {
        self.sync_now();
        let mut s = self.st.borrow_mut();
        let c = s.caret();
        let lay = &s.lay;
        c.para + 1 >= lay.paras.len()
            && c.line + 1 >= lay.paras.get(c.para).map_or(1, |p| p.layout.lines.len())
    }

    /// The caret in window coordinates (x, y, w, h) (completion popups,
    /// tests), clipped to the visible text box.
    pub fn caret_rect(&self) -> (i32, i32, i32, i32) {
        self.caret_rect_at(None)
    }

    /// Window rect of the caret placed at byte `pos` (without moving it).
    pub fn caret_rect_at(&self, pos: Option<usize>) -> (i32, i32, i32, i32) {
        self.sync_now();
        let (tx, ty, _, th) = text_box(
            self.inner.x(),
            self.inner.y(),
            self.inner.w(),
            self.inner.h(),
        );
        let mut s = self.st.borrow_mut();
        let c = match pos {
            Some(p) => {
                let p = p.min(s.buf.len());
                let s = &mut *s;
                s.lay.caret(s.buf.text(), p, false, &mut s.m)
            }
            None => s.caret(),
        };
        let y = (ty + c.y - s.scroll).clamp(ty, ty + th - 1);
        (tx + c.x, y, 2, c.h.min(ty + th - y))
    }

    /// The window rectangle of the first occurrence of `needle` (tests).
    pub fn text_rect(&self, needle: &str) -> Option<(i32, i32, i32, i32)> {
        let at = self.st.borrow().buf.text().find(needle)?;
        let (x0, y, _, h) = self.caret_rect_at(Some(at));
        let (x1, _, _, _) = self.caret_rect_at(Some(at + needle.len()));
        Some((x0, y, (x1 - x0).max(1), h))
    }

    /// The height the area wants now (chrome + clamped content).
    pub fn natural_h(&self) -> i32 {
        let (_, py) = pads();
        if self.inner.w() > 0 {
            self.sync_now();
        }
        let s = self.st.borrow();
        let ch = if s.lay.paras.is_empty() {
            Inner::typo().line_h(Inner::base_font().size) * s.lines.0.max(1) as i32
        } else {
            s.content_h()
        };
        ch + 2 * py + 2
    }

    pub fn preferred_size(&self) -> (i32, i32) {
        (self.inner.w(), self.natural_h())
    }

    // ------------------------------------------------------------ editing API

    /// Replaces the text (one undo step; caret at the end).
    pub fn set_value(&self, text: &str) {
        self.edit(|b| b.set_text(text));
    }

    /// Replaces the text and forgets the undo history.
    pub fn reset(&self, text: &str) {
        self.edit(|b| b.reset(text));
    }

    pub fn clear_history(&self) {
        self.st.borrow_mut().buf.clear_history();
    }

    /// Inserts `text` as if typed (replaces the selection; typing groups).
    pub fn type_text(&self, text: &str) {
        self.edit(|b| b.insert(text));
    }

    /// Inserts `text` at the caret as one undo step (paste).
    pub fn insert(&self, text: &str) {
        self.edit(|b| b.paste(text));
    }

    /// Inserts a chip `text` carrying `data` at the caret (replacing the selection).
    pub fn insert_chip(&self, text: &str, data: &str) {
        self.edit(|b| {
            let r = b.selection();
            b.insert_chip(r, text, data);
        });
    }

    /// Replaces bytes `range` with a chip.
    pub fn replace_with_chip(&self, range: Range<usize>, text: &str, data: &str) {
        self.edit(|b| b.insert_chip(range, text, data));
    }

    /// Replaces bytes `range` with `text` (one undo step).
    pub fn replace(&self, range: Range<usize>, text: &str) {
        self.edit(|b| {
            b.replace(range, text, Vec::new(), super::EditKind::Other);
            b.break_group();
        });
    }

    pub fn set_chips(&self, chips: Vec<Chip>) {
        self.edit(|b| b.set_chips(chips));
    }

    /// Selects bytes `anchor..caret` (the caret is the moving end).
    pub fn select(&self, anchor: usize, caret: usize) {
        self.edit(|b| b.select(anchor, caret));
    }

    pub fn select_all(&self) {
        self.edit(|b| b.select_all());
    }

    /// Moves the caret (no selection).
    pub fn set_caret(&self, pos: usize) {
        self.edit(|b| b.move_to(pos, false));
    }

    pub fn undo(&self) -> bool {
        self.edit(|b| b.undo())
    }

    pub fn redo(&self) -> bool {
        self.edit(|b| b.redo())
    }

    /// Copies the selection to the clipboard.
    pub fn copy(&self) {
        let t = self.selected_text();
        if !t.is_empty() {
            app::copy(&t);
        }
    }

    pub fn cut(&self) {
        if self.st.borrow().read_only {
            return self.copy();
        }
        self.copy();
        self.edit(|b| {
            if b.has_selection() {
                b.backspace();
            }
        });
    }

    /// Pastes the clipboard (arrives as a paste event).
    pub fn paste(&self) {
        if !self.st.borrow().read_only {
            app::paste_text(&self.inner);
        }
    }

    // ------------------------------------------------------------ internals

    fn text_w(&self) -> i32 {
        text_box(0, 0, self.inner.w(), self.inner.h()).2
    }

    fn view_h(&self) -> i32 {
        text_box(0, 0, self.inner.w(), self.inner.h()).3
    }

    fn sync_now(&self) {
        let w = self.text_w();
        self.st.borrow_mut().sync(w);
    }

    /// Runs `f` on the buffer, then lays out, reveals the caret, resizes,
    /// redraws and notifies (callbacks run with no borrow held).
    pub(crate) fn edit<R>(&self, f: impl FnOnce(&mut EditBuffer) -> R) -> R {
        let (r, changed) = {
            let mut s = self.st.borrow_mut();
            let rev = s.buf.revision();
            let r = f(&mut s.buf);
            s.goal_x = None;
            (r, s.buf.revision() != rev)
        };
        self.after(changed, true);
        r
    }

    /// After an edit or a move: layout, size, scroll, redraw, callbacks.
    pub(crate) fn after(&self, text_changed: bool, reveal: bool) {
        let w = self.text_w();
        let natural = {
            let mut s = self.st.borrow_mut();
            s.sync(w);
            s.content_h() + 2 * pads().1 + 2
        };
        if natural != self.inner.h() {
            self.set_height(natural);
        }
        {
            let vh = self.view_h();
            let mut s = self.st.borrow_mut();
            if reveal {
                s.reveal(vh);
            } else {
                s.clamp_scroll(vh);
            }
        }
        self.inner.clone().redraw();
        if text_changed && self.cbs.change.is_set() {
            let t = self.value();
            self.cbs.change.call(t);
        }
        self.cbs.caret.call(());
    }

    fn set_height(&self, h: i32) {
        if self.cbs.height.is_set() {
            self.cbs.height.call(h);
            return;
        }
        let mut w = self.inner.clone();
        if !crate::layout::refit(&w, (w.w(), h)) {
            w.set_size(w.w(), h);
            if let Some(mut p) = w.parent() {
                p.redraw();
            }
        }
    }

    /// Window coordinates → layout coordinates.
    fn to_layout(&self, ex: i32, ey: i32) -> (i32, i32) {
        let (tx, ty, _, _) = text_box(
            self.inner.x(),
            self.inner.y(),
            self.inner.w(),
            self.inner.h(),
        );
        let s = self.st.borrow();
        (ex - tx, ey - ty + s.scroll)
    }

    fn hit(&self, ex: i32, ey: i32) -> (usize, bool) {
        self.sync_now();
        let (lx, ly) = self.to_layout(ex, ey);
        let mut s = self.st.borrow_mut();
        let s = &mut *s;
        s.lay.hit(s.buf.text(), lx, ly, &mut s.m)
    }

    /// Up/Down by `n` lines (PageUp/PageDown); false when nothing moved.
    fn vertical(&self, down: bool, n: usize, extend: bool) -> bool {
        self.sync_now();
        let moved = {
            let mut s = self.st.borrow_mut();
            let s = &mut *s;
            let c = s.caret();
            let goal = s.goal_x.unwrap_or(c.x);
            let (mut pi, mut li) = (c.para, c.line);
            let mut steps = 0;
            for _ in 0..n.max(1) {
                match s.lay.neighbour(pi, li, down) {
                    Some(x) => {
                        (pi, li) = x;
                        steps += 1;
                    }
                    None => break,
                }
            }
            let before = (s.buf.caret(), s.buf.anchor());
            let (pos, up) = if steps < n.max(1) && (steps == 0 || n > 1) {
                (if down { s.buf.len() } else { 0 }, false)
            } else {
                s.lay.hit_line(s.buf.text(), pi, li, goal, &mut s.m)
            };
            s.buf.move_to(pos, extend);
            s.upstream = up;
            s.goal_x = Some(goal);
            (s.buf.caret(), s.buf.anchor()) != before
        };
        self.after(false, true);
        moved
    }

    /// Home/End of the visual line.
    fn line_edge(&self, end: bool, extend: bool) {
        self.sync_now();
        {
            let mut s = self.st.borrow_mut();
            let s = &mut *s;
            let c = s.caret();
            let r = s.lay.line_range(c.para, c.line);
            let last = c.line + 1 >= s.lay.paras[c.para].layout.lines.len();
            let (pos, up) = if !end {
                (r.start, false)
            } else if last {
                (r.end, false)
            } else {
                match s.buf.text()[r.clone()].chars().next_back() {
                    Some(ch) if ch.is_whitespace() => (r.end - ch.len_utf8(), false),
                    _ => (r.end, true),
                }
            };
            s.buf.move_to(pos, extend);
            s.upstream = up;
            s.goal_x = None;
        }
        self.after(false, true);
    }

    fn mv(&self, f: impl FnOnce(&mut EditBuffer)) -> bool {
        let before = {
            let mut s = self.st.borrow_mut();
            let b = (s.buf.caret(), s.buf.anchor());
            f(&mut s.buf);
            s.upstream = false;
            s.goal_x = None;
            b
        };
        self.after(false, true);
        let s = self.st.borrow();
        (s.buf.caret(), s.buf.anchor()) != before
    }

    fn editable(&self) -> bool {
        !self.st.borrow().read_only && self.inner.active_r()
    }

    /// Normalized pasted text: CRLF/CR → LF, other control chars dropped.
    fn clean(text: &str) -> String {
        let t = text.replace("\r\n", "\n").replace('\r', "\n");
        t.chars()
            .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
            .collect()
    }

    fn handle(&self, ev: Event) -> bool {
        let mut w = self.inner.clone();
        match ev {
            Event::Focus => {
                w.redraw();
                self.cbs.focus.call(true);
                true
            }
            Event::Unfocus => {
                {
                    let mut s = self.st.borrow_mut();
                    if s.buf.preedit().is_some() {
                        s.buf.commit_preedit();
                        app::compose_reset();
                    }
                    s.buf.break_group();
                    s.drag = None;
                }
                draw::reset_spot();
                w.redraw();
                self.cbs.focus.call(false);
                true
            }
            Event::Enter => {
                self.st.borrow_mut().hover = true;
                draw::set_cursor(Cursor::Insert);
                w.redraw();
                true
            }
            Event::Move => {
                let (ex, _) = app::event_coords();
                let over_bar = self.over_scrollbar(ex);
                draw::set_cursor(if over_bar {
                    Cursor::Default
                } else {
                    Cursor::Insert
                });
                true
            }
            Event::Leave => {
                self.st.borrow_mut().hover = false;
                draw::set_cursor(Cursor::Default);
                w.redraw();
                true
            }
            Event::Push => self.push(),
            Event::Drag => {
                self.drag_to();
                true
            }
            Event::Released => {
                let had = self.st.borrow_mut().drag.take().is_some();
                let sel = self.selected_text();
                if had && !sel.is_empty() {
                    app::copy2(&sel);
                }
                true
            }
            Event::MouseWheel => {
                let vh = self.view_h();
                let lh = self.st.borrow().lay.line_h;
                let scrollable = self.st.borrow().lay.height > vh;
                if !scrollable {
                    return false;
                }
                let dy = app::event_dy_value();
                {
                    let mut s = self.st.borrow_mut();
                    s.scroll += dy * lh * 2;
                    s.clamp_scroll(vh);
                }
                w.redraw();
                true
            }
            Event::KeyDown => self.key(),
            Event::Paste => {
                let dropped = std::mem::take(&mut self.st.borrow_mut().dnd);
                let text = app::event_text();
                if self.cbs.paste.call((text.clone(), dropped)) == Some(true) {
                    return true;
                }
                if !self.editable() {
                    return true;
                }
                let t = Self::clean(&text);
                if !t.is_empty() {
                    if dropped {
                        let (ex, ey) = app::event_coords();
                        let (p, _) = self.hit(ex, ey);
                        if !self.selection().contains(&p) {
                            self.edit(|b| b.move_to(p, false));
                        }
                    }
                    self.edit(|b| b.paste(&t));
                }
                true
            }
            Event::DndEnter | Event::DndDrag | Event::DndLeave => {
                self.editable() || self.cbs.paste.is_set()
            }
            Event::DndRelease => {
                self.st.borrow_mut().dnd = true;
                true
            }
            Event::Shortcut => false,
            _ => false,
        }
    }

    fn scrollbar(&self) -> Option<(i32, i32, i32, i32)> {
        let (x, y, w, h) = (
            self.inner.x(),
            self.inner.y(),
            self.inner.w(),
            self.inner.h(),
        );
        let (_, ty, _, th) = text_box(x, y, w, h);
        let (content, scroll) = {
            let s = self.st.borrow();
            (s.lay.height, s.scroll)
        };
        if content <= th {
            return None;
        }
        let (px, _) = pads();
        let bx = x + w - px / 2 - SCROLLBAR / 2 - 1;
        let thumb = ((th as f64 / content as f64) * th as f64)
            .max(16.0)
            .min(th as f64) as i32;
        let max = (content - th).max(1);
        let tyy = ty + ((scroll as f64 / max as f64) * (th - thumb) as f64) as i32;
        Some((bx, tyy, SCROLLBAR - 2, thumb))
    }

    fn over_scrollbar(&self, ex: i32) -> bool {
        self.scrollbar()
            .is_some_and(|(bx, _, bw, _)| ex >= bx - 3 && ex <= bx + bw + 3)
    }

    fn push(&self) -> bool {
        let mut w = self.inner.clone();
        let (ex, ey) = app::event_coords();
        let button = app::event_mouse_button();
        if !crate::focus::is_focused(&w) {
            let _ = w.take_focus();
        }
        if button == app::MouseButton::Right {
            self.context_menu(ex, ey);
            return true;
        }
        if let Some((_, by, _, bh)) = self.scrollbar().filter(|_| self.over_scrollbar(ex)) {
            let off = if ey >= by && ey < by + bh {
                ey - by
            } else {
                bh / 2
            };
            self.st.borrow_mut().drag = Some(Drag::Bar(off));
            self.drag_to();
            return true;
        }
        let (pos, up) = self.hit(ex, ey);
        if button == app::MouseButton::Middle {
            if self.editable() {
                self.edit(|b| b.move_to(pos, false));
                app::paste_text2(&w);
            }
            return true;
        }
        let shift = app::event_state().contains(Shortcut::Shift);
        let clicks = if app::event_clicks() {
            app::event_clicks_num()
        } else {
            0
        };
        {
            let mut s = self.st.borrow_mut();
            if s.buf.preedit().is_some() {
                s.buf.commit_preedit();
                app::compose_reset();
            }
        }
        match clicks {
            0 => {
                self.edit(|b| b.move_to(pos, shift));
                let mut s = self.st.borrow_mut();
                s.upstream = up;
                s.drag = Some(Drag::Chars);
            }
            1 => {
                let r = self.st.borrow().buf.word_at(pos);
                self.edit(|b| b.select(r.start, r.end));
                self.st.borrow_mut().drag = Some(Drag::Words(r.start, r.end));
            }
            _ => {
                let (s0, e0) = {
                    let s = self.st.borrow();
                    (s.buf.para_start(pos), s.buf.para_end(pos))
                };
                let e1 = (e0 + 1).min(self.st.borrow().buf.len());
                self.edit(|b| b.select(s0, e1));
                self.st.borrow_mut().drag = Some(Drag::Paras(s0, e1));
            }
        }
        w.redraw();
        true
    }

    fn drag_to(&self) {
        let (ex, ey) = app::event_coords();
        let Some(mode) = self.st.borrow().drag else {
            return;
        };
        let (_, ty, _, th) = text_box(
            self.inner.x(),
            self.inner.y(),
            self.inner.w(),
            self.inner.h(),
        );
        if let Drag::Bar(off) = mode {
            let content = self.st.borrow().lay.height;
            if let Some((_, _, _, thumb)) = self.scrollbar() {
                let track = (th - thumb).max(1);
                let frac = ((ey - off - ty) as f64 / track as f64).clamp(0.0, 1.0);
                let mut s = self.st.borrow_mut();
                s.scroll = (frac * (content - th).max(0) as f64).round() as i32;
                s.clamp_scroll(th);
            }
            self.inner.clone().redraw();
            return;
        }
        // Dragging above or below the text scrolls one line per event.
        {
            let mut s = self.st.borrow_mut();
            let lh = s.lay.line_h;
            if ey < ty {
                s.scroll -= lh;
            } else if ey > ty + th {
                s.scroll += lh;
            }
            s.clamp_scroll(th);
        }
        let (pos, up) = self.hit(ex, ey.clamp(ty, ty + th - 1));
        match mode {
            Drag::Chars => {
                let anchor = self.st.borrow().buf.anchor();
                self.edit(|b| b.select(anchor, pos));
                self.st.borrow_mut().upstream = up;
            }
            Drag::Words(s0, e0) => {
                let r = self.st.borrow().buf.word_at(pos);
                if pos < s0 {
                    self.edit(|b| b.select(e0, r.start));
                } else {
                    self.edit(|b| b.select(s0, r.end.max(e0)));
                }
            }
            Drag::Paras(s0, e0) => {
                let (ps, pe) = {
                    let s = self.st.borrow();
                    (
                        s.buf.para_start(pos),
                        (s.buf.para_end(pos) + 1).min(s.buf.len()),
                    )
                };
                if pos < s0 {
                    self.edit(|b| b.select(e0, ps));
                } else {
                    self.edit(|b| b.select(s0, pe.max(e0)));
                }
            }
            Drag::Bar(_) => {}
        }
    }

    fn context_menu(&self, x: i32, y: i32) {
        use crate::popups::{PopupItem, PopupMenu};
        use crate::tr;
        let (can_undo, can_redo, sel, editable) = {
            let s = self.st.borrow();
            (
                s.buf.can_undo(),
                s.buf.can_redo(),
                s.buf.has_selection(),
                !s.read_only,
            )
        };
        let item = |label: String, spec: &str, on: bool| {
            let it = PopupItem::new("", &label).shortcut(spec);
            if on {
                it
            } else {
                it.disabled()
            }
        };
        let items = vec![
            item(tr!("Undo"), "Mod+Z", can_undo && editable),
            item(tr!("Redo"), "Mod+Shift+Z", can_redo && editable).divider(),
            item(tr!("Cut"), "Mod+X", sel && editable),
            item(tr!("Copy"), "Mod+C", sel),
            item(tr!("Paste"), "Mod+V", editable).divider(),
            item(tr!("Select all"), "Mod+A", true),
        ];
        let me = self.clone();
        PopupMenu::open_at(&self.inner, x, y, items, move |i| match i {
            0 => {
                me.undo();
            }
            1 => {
                me.redo();
            }
            2 => me.cut(),
            3 => me.copy(),
            4 => me.paste(),
            _ => me.select_all(),
        });
    }

    fn key(&self) -> bool {
        let k = app::event_key();
        let mods = app::event_state()
            & (Shortcut::Ctrl | Shortcut::Shift | Shortcut::Alt | Shortcut::Meta);
        if self.cbs.pre_key.call((k, mods)) == Some(true) {
            return true;
        }
        if self.cbs.key.call((k, mods)) == Some(true) {
            return true;
        }
        let editable = self.editable();
        // Text from the keyboard or an input method (dead keys, compose,
        // preedit): FLTK says how many bytes before the caret it replaces.
        if let Some(del) = app::compose() {
            if !editable {
                app::compose_reset();
                return false;
            }
            let text: String = app::event_text()
                .chars()
                .filter(|c| !c.is_control() || *c == '\t')
                .collect();
            let marked = app::compose_state().max(0) as usize;
            if del == 0 && text.is_empty() && marked == 0 {
                // A lone control char (Enter while composing): commit.
                let had = self.st.borrow().buf.preedit().is_some();
                self.st.borrow_mut().buf.commit_preedit();
                if had {
                    self.inner.clone().redraw();
                    return true;
                }
                return false;
            }
            let has_marked = OS != Os::Windows && marked > 0;
            self.edit(|b| {
                b.compose(
                    del.max(0) as usize,
                    &text,
                    if has_marked { marked } else { 0 },
                )
            });
            return true;
        }
        // Any other key ends a composition.
        if self.st.borrow().buf.preedit().is_some() {
            self.st.borrow_mut().buf.commit_preedit();
        }
        let shift = mods.contains(Shortcut::Shift);
        let primary = crate::keys::is_primary(mods);
        let mac = OS == Os::MacOs;
        let ctrl = mods.contains(Shortcut::Ctrl);
        let alt = mods.contains(Shortcut::Alt);
        let meta = mods.contains(Shortcut::Meta);
        // Word moves: Ctrl elsewhere, Alt on macOS.
        let word = if mac {
            alt && !ctrl && !meta
        } else {
            ctrl && !alt && !meta
        };
        let plain = !ctrl && !alt && !meta;
        let letter =
            |c: char| k == Key::from_char(c) || k == Key::from_char(c.to_ascii_uppercase());
        match k {
            Key::Left | Key::Right => {
                let fwd = k == Key::Right;
                if mac && meta && !ctrl && !alt {
                    self.line_edge(fwd, shift);
                    true
                } else if word {
                    let used = self.mv(|b| {
                        let p = if fwd {
                            b.word_right(b.caret())
                        } else {
                            b.word_left(b.caret())
                        };
                        b.move_to(p, shift);
                    });
                    used || !shift
                } else if plain {
                    self.mv(|b| b.step(fwd, shift))
                } else {
                    false
                }
            }
            Key::Up | Key::Down => {
                let down = k == Key::Down;
                if mac && meta && !ctrl && !alt {
                    self.mv(|b| b.move_to(if down { b.len() } else { 0 }, shift))
                } else if plain {
                    self.vertical(down, 1, shift)
                } else {
                    false
                }
            }
            Key::PageUp | Key::PageDown if plain => {
                let lh = self.st.borrow().lay.line_h.max(1);
                let n = ((self.view_h() / lh) - 1).max(1) as usize;
                self.vertical(k == Key::PageDown, n, shift)
            }
            Key::Home | Key::End => {
                let end = k == Key::End;
                if (ctrl && !mac) || (meta && mac) {
                    self.mv(|b| b.move_to(if end { b.len() } else { 0 }, shift));
                } else if plain {
                    self.line_edge(end, shift);
                } else {
                    return false;
                }
                true
            }
            Key::BackSpace if editable => {
                if mac && meta && !alt && !ctrl {
                    let start = self.line_start_of_caret();
                    self.edit(|b| b.delete_to(start));
                } else if word {
                    self.edit(|b| {
                        let p = b.word_left(b.caret());
                        b.delete_to(p);
                    });
                } else if plain || shift && !ctrl && !alt && !meta {
                    self.edit(|b| b.backspace());
                } else {
                    return false;
                }
                true
            }
            Key::Delete if editable => {
                if word {
                    self.edit(|b| {
                        let p = b.word_right(b.caret());
                        b.delete_to(p);
                    });
                } else if shift && plain && !mac {
                    self.cut();
                } else if plain {
                    self.edit(|b| b.delete_forward());
                } else {
                    return false;
                }
                true
            }
            Key::Insert if !mac && ctrl && !shift && !alt => {
                self.copy();
                true
            }
            Key::Insert if !mac && shift && plain && editable => {
                self.paste();
                true
            }
            Key::Enter | Key::KPEnter if editable && !ctrl && !alt && !meta => {
                self.edit(|b| {
                    b.break_group();
                    b.insert("\n");
                    b.break_group();
                });
                true
            }
            Key::Tab => {
                let tab = self.st.borrow().tab;
                let insert = editable
                    && match tab {
                        TabKey::Leave => false,
                        TabKey::CtrlTabInserts => ctrl && !shift && !alt && !meta,
                        TabKey::Inserts => plain && !shift,
                    };
                if insert {
                    self.edit(|b| b.insert("\t"));
                }
                insert
            }
            Key::Escape if plain && !shift => {
                if self.st.borrow().buf.has_selection() {
                    self.mv(|b| {
                        let c = b.caret();
                        b.move_to(c, false);
                    });
                    return true;
                }
                false
            }
            _ if primary && !alt && letter('a') && !shift => {
                self.select_all();
                true
            }
            _ if primary && !alt && letter('c') && !shift => {
                self.copy();
                true
            }
            _ if primary && !alt && letter('x') && !shift => {
                self.cut();
                true
            }
            _ if primary && !alt && letter('v') && !shift => {
                self.paste();
                true
            }
            _ if primary && !alt && letter('z') && editable => {
                if shift {
                    self.redo();
                } else {
                    self.undo();
                }
                true
            }
            _ if primary && !alt && !shift && !mac && letter('y') && editable => {
                self.redo();
                true
            }
            _ => false,
        }
    }

    fn line_start_of_caret(&self) -> usize {
        self.sync_now();
        let mut s = self.st.borrow_mut();
        let c = s.caret();
        s.lay.line_range(c.para, c.line).start
    }

    // ------------------------------------------------------------ drawing

    fn draw(&self, w: &mut Widget) {
        let (x, y, ww, wh) = (w.x(), w.y(), w.w(), w.h());
        let (tx, ty, tw, th) = text_box(x, y, ww, wh);
        let focused = crate::focus::is_focused(w);
        let active = w.active_r();
        let mut guard = self.st.borrow_mut();
        guard.sync(tw);
        guard.clamp_scroll(th);
        let s = &mut *guard;
        theme::with(|t| {
            let under = t.palette.background;
            let st = if focused {
                &t.controls.focus
            } else if s.hover {
                &t.controls.hover
            } else {
                &t.controls.normal
            };
            let fill = t.fill(st, under);
            paint::surface(
                x,
                y,
                ww,
                wh,
                t.radius,
                Some(fill),
                Some((t.border(st, under), st.border_width)),
            );
            let fg = if active {
                t.palette.foreground
            } else {
                t.palette.foreground.darker(2.0)
            };
            let sel = t.palette.foreground.over(
                fill,
                t.controls.selection_fill_alpha * if focused { 1.0 } else { 0.6 },
            );
            let chip_bg = t.palette.accent.over(fill, 0.16);
            draw::push_clip(tx - 2, ty, tw + 4, th);
            let oy = ty - s.scroll;
            let text = s.buf.text();
            let m = &mut s.m;
            let lay = &s.lay;
            // Selection.
            let r = s.buf.selection();
            if !r.is_empty() {
                for (rx, ry, rw, rh) in lay.rects(text, r.clone(), s.scroll, s.scroll + th, m) {
                    paint::fill(tx + rx, oy + ry, rw, rh, 0, sel);
                }
            }
            let pre = s.buf.preedit();
            let first = lay.paras.partition_point(|p| p.y + p.h() < s.scroll);
            for (pi, p) in lay.paras.iter().enumerate().skip(first) {
                if p.y > s.scroll + th {
                    break;
                }
                let ptext = &text[p.start..p.start + p.len];
                for l in &p.layout.lines {
                    let ly = oy + p.y + l.y;
                    if ly > ty + th || ly + l.h < ty {
                        continue;
                    }
                    let base_y = ly + l.base;
                    for f in p.layout.frags_of(l) {
                        if f.tab {
                            continue;
                        }
                        let fx = tx + l.x + f.x.round() as i32;
                        let fw = f.w.ceil() as i32;
                        let chip = p.chip_runs.get(f.run as usize).copied().unwrap_or(false);
                        if chip {
                            paint::fill(
                                fx - 2,
                                ly + 1,
                                fw + 4,
                                l.h - 2,
                                (t.radius).min(4),
                                chip_bg,
                            );
                        }
                        FltkMetrics::set(f.font);
                        draw::set_draw_color(if chip { t.palette.accent } else { fg }.fl());
                        draw::draw_text_n(&ptext[f.start as usize..f.end as usize], fx, base_y);
                    }
                    // Uncommitted input-method text is underlined.
                    if let Some(pr) = &pre {
                        let (ls, le) = (p.start + l.start as usize, p.start + l.end as usize);
                        let (a, b) = (pr.start.max(ls), pr.end.min(le));
                        if a < b {
                            let li = p
                                .layout
                                .lines
                                .iter()
                                .position(|x| std::ptr::eq(x, l))
                                .unwrap_or(0);
                            let x0 = lay.x_at(text, pi, li, a - p.start, m);
                            let x1 = lay.x_at(text, pi, li, b - p.start, m);
                            paint::fill(
                                tx + x0 as i32,
                                base_y + 2,
                                (x1 - x0).ceil() as i32,
                                1,
                                0,
                                fg,
                            );
                        }
                    }
                }
            }
            if text.is_empty() && !s.placeholder.is_empty() {
                let ph = t.palette.foreground.darker(1.6);
                let align = if lay.rtl { Align::Right } else { Align::Left };
                draw::set_font(paint::font(false), t.font.body);
                draw::set_draw_color(ph.fl());
                let lh = lay.line_h;
                draw::draw_text2(
                    &paint::escape(&s.placeholder),
                    tx,
                    ty,
                    tw,
                    th.max(lh),
                    align | Align::Top | Align::Wrap | Align::Inside | Align::Clip,
                );
            }
            // Caret (no blinking: an idle editor costs nothing).
            let c = lay.caret(text, s.buf.caret(), s.upstream, m);
            if focused && active && (r.is_empty() || pre.is_some()) {
                let cx = (tx + c.x).min(tx + tw);
                paint::fill(cx - 1, oy + c.y + 1, 2, c.h - 2, 0, t.palette.foreground);
            }
            draw::pop_clip();
            if focused {
                // Where input methods put their candidate window.
                if let Some(win) = w.window() {
                    // SAFETY: a live window pointer from fltk.
                    let win = unsafe { fltk::window::Window::from_widget_ptr(win.as_widget_ptr()) };
                    let cy = (oy + c.y + c.h).clamp(ty, ty + th);
                    draw::set_spot(paint::font(false), t.font.body, tx + c.x, cy, tw, th, &win);
                }
            }
        });
        drop(guard);
        if let Some((bx, by, bw, bh)) = self.scrollbar() {
            theme::with(|t| {
                let fill = t.fill(
                    if focused {
                        &t.controls.focus
                    } else {
                        &t.controls.normal
                    },
                    t.palette.background,
                );
                paint::fill(bx, by, bw, bh, bw / 2, t.palette.foreground.over(fill, 0.3));
            });
        }
    }

    // ------------------------------------------------------------ a11y

    fn describe(&self, n: &mut Node) {
        let s = self.st.borrow();
        let text = s.buf.text();
        n.role(Role::MultilineTextInput)
            .name(&s.placeholder)
            .placeholder(&s.placeholder)
            .value(text)
            .focusable()
            .action(Action::SetValue)
            .action(Action::SetTextSelection)
            .action(Action::ReplaceSelectedText)
            .text_selection(
                s.buf.char_index(s.buf.anchor()),
                s.buf.char_index(s.buf.caret()),
            );
        if s.read_only {
            n.read_only(true);
        }
        drop(s);
        let extra = self.cbs.a11y_extra.borrow().clone();
        if let Some(f) = extra {
            f(n);
        }
    }

    fn act(&self, r: &Request) -> bool {
        if !r.path.is_empty() {
            return self.cbs.a11y_action.call(r.clone()).unwrap_or(false);
        }
        match (r.action, &r.value) {
            (Action::Focus, _) => return self.inner.clone().take_focus().is_ok(),
            (Action::SetValue, Some(a11y::Value::Text(t))) if self.editable() => self.set_value(t),
            (Action::ReplaceSelectedText, Some(a11y::Value::Text(t))) if self.editable() => {
                self.insert(t)
            }
            (Action::SetTextSelection, Some(a11y::Value::Selection { anchor, focus })) => {
                let (a, f) = {
                    let s = self.st.borrow();
                    (s.buf.byte_index(*anchor), s.buf.byte_index(*focus))
                };
                self.select(a, f);
            }
            _ => return self.cbs.a11y_action.call(r.clone()).unwrap_or(false),
        }
        true
    }
}
