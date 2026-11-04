//! Virtualized list in Omarchy's row style. Only visible rows are built,
//! so 1 000 000 rows cost the same per frame as 20.
//!
//! ```no_run
//! use fltk_omarchy::{list::{List, ListRow}, State};
//! let count = State::new(100_000usize);
//! let selected = State::new(None::<usize>);
//! let list = List::new(&count, |i| ListRow::new(&format!("Item {i}")).detail(&format!("{} KB", i * 3)))
//!     .bind_selected(&selected)
//!     .on_activate(|i| println!("open {i}"));
//! ```

use crate::paint;
use crate::state::State;
use crate::theme;
use crate::widgets::draw_row;
use fltk::{app, enums::*, prelude::*, widget::Widget};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ListRow {
    pub icon: String,
    pub label: String,
    pub detail: String,
}

impl ListRow {
    pub fn new(label: &str) -> ListRow {
        ListRow { label: label.into(), ..Default::default() }
    }
    pub fn icon(mut self, g: &str) -> Self { self.icon = g.into(); self }
    pub fn detail(mut self, d: &str) -> Self { self.detail = d.into(); self }
}

struct Inner {
    row: Box<dyn Fn(usize) -> ListRow>,
    count: usize,
    top: i64, // scroll offset in px
    motion: crate::scroll::Scroll,
    row_h: i32,
    hover: Option<usize>,
    selected: Option<usize>,
    fetched: usize,
    dragging_thumb: Option<(i32, i64)>,
}

#[derive(Clone)]
pub struct List {
    widget: Widget,
    inner: Rc<RefCell<Inner>>,
    select_cb: Rc<RefCell<Option<Box<dyn FnMut(usize)>>>>,
    activate_cb: Rc<RefCell<Option<Box<dyn FnMut(usize)>>>>,
    selected_state: Rc<RefCell<Option<State<Option<usize>>>>>,
    activations: Rc<Cell<usize>>,
}
fltk::widget_extends!(List, Widget, widget);

const BAR: i32 = 4;

impl List {
    pub fn new(count: &State<usize>, row: impl Fn(usize) -> ListRow + 'static) -> List {
        let mut widget = Widget::default().with_size(240, 200);
        widget.set_frame(FrameType::NoBox);
        widget.set_visible_focus();
        let row_h = theme::with(|t| t.font.body + 4 + 2 * t.spacing.xl);
        let inner = Rc::new(RefCell::new(Inner {
            row: Box::new(row), count: count.get(), top: 0, motion: crate::scroll::Scroll::new(), row_h, hover: None, selected: None, fetched: 0, dragging_thumb: None,
        }));
        let list = List {
            widget,
            inner,
            select_cb: Rc::default(),
            activate_cb: Rc::default(),
            selected_state: Rc::default(),
            activations: Rc::default(),
        };
        let (inn, mut w) = (list.inner.clone(), list.widget.clone());
        crate::state::bind_widget(&list.widget, count, false, move |n| {
            let mut i = inn.borrow_mut();
            i.count = *n;
            if i.selected.is_some_and(|s| s >= *n) { i.selected = None; }
            drop(i);
            w.redraw();
        });
        list.install();
        list
    }

    fn install(&self) {
        let inn = self.inner.clone();
        let mut w = self.widget.clone();
        w.draw(move |w| {
            let mut i = inn.borrow_mut();
            theme::with(|t| {
                let under = t.palette.background;
                fltk::draw::push_clip(w.x(), w.y(), w.w(), w.h());
                let max_top = (i.count as i64 * i.row_h as i64 - w.h() as i64).max(0);
                let top = i.top;
                i.top = i.motion.position(top, max_top);
                if i.motion.running() { crate::anim::request_redraw(w); }
                let first = (i.top / i.row_h as i64) as usize;
                let visible = (w.h() / i.row_h + 2) as usize;
                let text_w = w.w() - BAR - 4;
                for k in 0..visible {
                    let idx = first + k;
                    if idx >= i.count { break; }
                    let ry = w.y() + (idx as i64 * i.row_h as i64 - i.top) as i32;
                    let r = (i.row)(idx);
                    i.fetched += 1;
                    draw_row(t, w.x(), ry, text_w, i.row_h, &r.icon, &r.label, &r.detail, i.hover == Some(idx), i.selected == Some(idx), under);
                }
                if max_top > 0 {
                    let track = w.h();
                    let thumb = ((w.h() as f64 / (i.count as f64 * i.row_h as f64)) * track as f64).max(24.0) as i32;
                    let ty = w.y() + ((i.top as f64 / max_top as f64) * (track - thumb) as f64) as i32;
                    paint::fill(w.x() + w.w() - BAR, ty, BAR, thumb, BAR / 2, t.palette.foreground.over(under, 0.25));
                }
                fltk::draw::pop_clip();
                if crate::focus::show_ring(w) {
                    // Ring on the cursor row, or around the list when there is none.
                    let ring = t.border(&t.controls.focus, under);
                    let bw = t.controls.focus.border_width.max(1);
                    let row = i.selected.map(|s| w.y() + (s as i64 * i.row_h as i64 - i.top) as i32)
                        .filter(|ry| *ry + i.row_h > w.y() && *ry < w.y() + w.h());
                    match row {
                        Some(ry) => {
                            fltk::draw::push_clip(w.x(), w.y(), w.w(), w.h());
                            paint::stroke(w.x(), ry, text_w, i.row_h, t.radius, bw, ring);
                            fltk::draw::pop_clip();
                        }
                        None => paint::stroke(w.x(), w.y(), w.w(), w.h(), t.radius, bw, ring),
                    }
                }
            });
        });

        let (inn, sel_cb, act_cb, sel_state, acts) =
            (self.inner.clone(), self.select_cb.clone(), self.activate_cb.clone(), self.selected_state.clone(), self.activations.clone());
        w.handle(move |w, ev| {
            let wy = w.y();
            let row_at = move |i: &Inner, ey: i32| -> Option<usize> {
                let idx = ((ey - wy) as i64 + i.top) / i.row_h as i64;
                (idx >= 0 && (idx as usize) < i.count).then_some(idx as usize)
            };
            let select = |idx: usize, w: &mut Widget| {
                {
                    let mut i = inn.borrow_mut();
                    i.selected = Some(idx);
                    let y = idx as i64 * i.row_h as i64;
                    if y < i.top { i.top = y } else if y + i.row_h as i64 > i.top + w.h() as i64 { i.top = y + i.row_h as i64 - w.h() as i64 }
                }
                if let Some(s) = sel_state.borrow().as_ref() { s.set(Some(idx)); }
                if let Some(f) = sel_cb.borrow_mut().as_mut() { f(idx); }
                w.redraw();
            };
            let activate = |idx: usize| {
                acts.set(acts.get() + 1);
                if let Some(f) = act_cb.borrow_mut().as_mut() { f(idx); }
            };
            match ev {
                Event::Enter => true,
                Event::Focus | Event::Unfocus => { w.redraw(); true }
                Event::Move => {
                    let h = row_at(&inn.borrow(), app::event_y());
                    if inn.borrow().hover != h { inn.borrow_mut().hover = h; w.redraw(); }
                    true
                }
                Event::Leave => { inn.borrow_mut().hover = None; w.redraw(); true }
                Event::Push => {
                    let _ = w.take_focus();
                    { let mut i = inn.borrow_mut(); let top = i.top; i.motion.jump(top); }
                    if app::event_x() >= w.x() + w.w() - BAR - 4 {
                        let top = inn.borrow().top;
                        inn.borrow_mut().dragging_thumb = Some((app::event_y(), top));
                        return true;
                    }
                    let idx = row_at(&inn.borrow(), app::event_y());
                    if let Some(idx) = idx {
                        select(idx, w);
                        if app::event_clicks() { activate(idx); }
                    }
                    true
                }
                Event::Drag => {
                    let d = inn.borrow().dragging_thumb;
                    if let Some((y0, top0)) = d {
                        let mut i = inn.borrow_mut();
                        let content = i.count as f64 * i.row_h as f64;
                        i.top = top0 + ((app::event_y() - y0) as f64 * content / w.h() as f64) as i64;
                        drop(i);
                        w.redraw();
                    }
                    true
                }
                Event::Released => { inn.borrow_mut().dragging_thumb = None; true }
                Event::MouseWheel => {
                    let mut i = inn.borrow_mut();
                    let max = (i.count as i64 * i.row_h as i64 - w.h() as i64).max(0);
                    let (top, delta) = (i.top, app::event_dy_value() as f64 * 3.0 * i.row_h as f64);
                    i.top = i.motion.wheel(top, delta, max);
                    drop(i);
                    w.redraw();
                    true
                }
                Event::KeyDown => {
                    { let mut i = inn.borrow_mut(); let top = i.top; i.motion.jump(top); }
                    let (count, cur, page) = { let i = inn.borrow(); (i.count, i.selected, (w.h() / i.row_h).max(1) as usize) };
                    if count == 0 { return false; }
                    let k = app::event_key();
                    let next = match k {
                        Key::Down => Some(cur.map_or(0, |c| (c + 1).min(count - 1))),
                        Key::Up => Some(cur.map_or(0, |c| c.saturating_sub(1))),
                        Key::PageDown => Some(cur.map_or(0, |c| (c + page).min(count - 1))),
                        Key::PageUp => Some(cur.map_or(0, |c| c.saturating_sub(page))),
                        Key::Home => Some(0),
                        Key::End => Some(count - 1),
                        _ if k == Key::from_char('j') => Some(cur.map_or(0, |c| (c + 1).min(count - 1))),
                        _ if k == Key::from_char('k') => Some(cur.map_or(0, |c| c.saturating_sub(1))),
                        Key::Enter | Key::KPEnter => { if let Some(c) = cur { activate(c); } return true; }
                        _ => None,
                    };
                    if let Some(n) = next { select(n, w); return true; }
                    false
                }
                _ => false,
            }
        });
    }

    pub fn row_height(self, px: i32) -> Self {
        self.inner.borrow_mut().row_h = px.max(8);
        self
    }

    pub fn on_select(self, f: impl FnMut(usize) + 'static) -> Self {
        *self.select_cb.borrow_mut() = Some(Box::new(f));
        self
    }

    /// Double click or Enter.
    pub fn on_activate(self, f: impl FnMut(usize) + 'static) -> Self {
        *self.activate_cb.borrow_mut() = Some(Box::new(f));
        self
    }

    /// Two-way selection binding.
    pub fn bind_selected(self, s: &State<Option<usize>>) -> Self {
        *self.selected_state.borrow_mut() = Some(s.clone());
        let (inn, mut w) = (self.inner.clone(), self.widget.clone());
        crate::state::bind_widget(&self.widget, s, true, move |v| {
            inn.borrow_mut().selected = *v;
            w.redraw();
        });
        self
    }

    pub fn selected(&self) -> Option<usize> {
        self.inner.borrow().selected
    }

    /// Selects and scrolls to `idx` (fires on_select).
    pub fn select(&mut self, idx: usize) {
        let row_h = self.inner.borrow().row_h as i64;
        {
            let mut i = self.inner.borrow_mut();
            if idx >= i.count { return; }
            i.selected = Some(idx);
            i.top = idx as i64 * row_h - (self.widget.h() as i64 - row_h) / 2;
        }
        if let Some(s) = self.selected_state.borrow().as_ref() { s.set(Some(idx)); }
        if let Some(f) = self.select_cb.borrow_mut().as_mut() { f(idx); }
        self.widget.redraw();
    }

    pub fn activate_selected(&self) {
        if let Some(i) = self.selected() {
            self.activations.set(self.activations.get() + 1);
            if let Some(f) = self.activate_cb.borrow_mut().as_mut() { f(i); }
        }
    }

    /// Total rows built for drawing so far (proves virtualization in tests).
    pub fn rows_built(&self) -> usize {
        self.inner.borrow().fetched
    }

    pub fn count(&self) -> usize {
        self.inner.borrow().count
    }
}
