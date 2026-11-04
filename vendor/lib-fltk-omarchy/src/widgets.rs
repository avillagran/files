//! The classic Omarchy controls (omarchy-shell `Ui/` kit), drawn with the
//! live theme tokens. Every component derefs to an fltk widget, so it can be
//! placed in any `Flex`/`Group` and sized with fltk-rs as usual; each one
//! also reports its Omarchy implicit size via `.preferred_size()`.

use crate::anim::{Anim, Ease, Lerp};
use crate::paint::{self, Interaction};
use crate::state::State;
use crate::theme::{self, Rgb};
use fltk::{app, draw, enums::*, prelude::*, widget::Widget};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub mod glyph {
    //! Nerd Font glyphs used by Omarchy.
    pub const CHEVRON_DOWN: &str = "\u{f0140}";
    pub const CHEVRON_LEFT: &str = "\u{f0141}";
    pub const CHEVRON_UP: &str = "\u{f0143}";
    pub const CHEVRON_RIGHT: &str = "\u{f0142}";
    pub const REFRESH: &str = "\u{f0450}";
    pub const LOADING: &str = "\u{f0996}";
    pub const EMPTY: &str = "\u{f0209}";
    pub const CHECK: &str = "\u{f012c}";
    pub const CLOSE_CIRCLE: &str = "\u{f0159}";
    pub const SETTINGS: &str = "\u{f0493}";
    pub const BLUETOOTH: &str = "\u{f00af}";
    pub const WIFI: &str = "\u{f05a9}";
    pub const BELL: &str = "\u{f009a}";
    pub const VOLUME_HIGH: &str = "\u{f057e}";
    pub const VOLUME_MUTED: &str = "\u{f0581}";
    pub const HEADPHONES: &str = "\u{f02cb}";
    pub const MICROPHONE: &str = "\u{f036c}";
    pub const MONITOR: &str = "\u{f0379}";
    pub const PLUS: &str = "\u{f0415}";
    pub const PLAY: &str = "\u{f040a}";
    pub const STOP: &str = "\u{f04db}";
    pub const TRASH: &str = "\u{f01b4}";
    pub const SEARCH: &str = "\u{f0349}";
    pub const FOLDER: &str = "\u{f024b}";
    pub const FILE: &str = "\u{f0214}";
    pub const WINDOW_FLOAT: &str = "\u{f05b2}";
    pub const CLOSE: &str = "\u{f0156}";
    pub const CHECKMARK: &str = "\u{2713}";
    pub const CROSS: &str = "\u{2715}";
    pub const SUBMENU: &str = "\u{203a}";
}

type Cb<T> = Rc<RefCell<Option<Box<dyn FnMut(T)>>>>;

fn cb<T>() -> Cb<T> {
    Rc::new(RefCell::new(None))
}

fn fire<T>(c: &Cb<T>, v: T) {
    // Take the callback out while it runs so it may re-borrow the widget.
    let f = c.borrow_mut().take();
    if let Some(mut f) = f {
        f(v);
        let mut slot = c.borrow_mut();
        if slot.is_none() {
            *slot = Some(f);
        }
    }
}

/// Background the control sits on (window background unless inside a card).
fn under() -> Rgb {
    theme::with(|t| t.palette.background)
}

/// Generic hover/press/focus tracking. `on_activate` runs on click or on
/// Enter/Space when focused (if `keyboard`).
fn track(w: &mut Widget, st: Rc<RefCell<Interaction>>, keyboard: bool, on_activate: Rc<dyn Fn()>) {
    track_with_shortcut(w, st, keyboard, on_activate, Rc::new(|| None));
}

fn track_with_shortcut(w: &mut Widget, st: Rc<RefCell<Interaction>>, keyboard: bool, on_activate: Rc<dyn Fn()>,
                       shortcut: Rc<dyn Fn() -> Option<crate::keys::Keys>>) {
    if keyboard {
        w.set_visible_focus();
    } else {
        w.clear_visible_focus();
    }
    w.handle(move |w, ev| {
        let mut s = st.borrow_mut();
        let changed = match ev {
            Event::Enter => { s.hover = true; true }
            Event::Leave => { s.hover = false; s.pressed = false; true }
            Event::Push if app::event_mouse_button() == app::MouseButton::Left => {
                s.pressed = true;
                // take_focus sends Event::Focus to this same handler right away:
                // release the state first.
                drop(s);
                if keyboard { let _ = w.take_focus(); }
                w.redraw();
                return true;
            }
            Event::Released if s.pressed => {
                s.pressed = false;
                let inside = app::event_inside_widget(w);
                drop(s);
                w.redraw();
                if inside && w.active() { on_activate(); }
                return true;
            }
            Event::Focus if keyboard => { s.focus = true; true }
            Event::Unfocus => { s.focus = false; true }
            Event::Shortcut if shortcut().is_some_and(|k| k.matches_event()) && w.active_r() => {
                drop(s);
                on_activate();
                return true;
            }
            Event::KeyDown if keyboard && s.focus => {
                let k = app::event_key();
                if k == Key::Enter || k == Key::KPEnter || k == Key::from_char(' ') {
                    drop(s);
                    if w.active() { on_activate(); }
                    return true;
                }
                false
            }
            _ => false,
        };
        if changed {
            drop(s);
            w.redraw();
            return true;
        }
        matches!(ev, Event::Enter | Event::Leave)
    });
}

fn interaction(w: &Widget, st: &Rc<RefCell<Interaction>>) -> Interaction {
    let mut s = *st.borrow();
    s.focus &= crate::focus::keyboard_mode();
    s.disabled = !w.active_r();
    s
}

// ================================================================== Button

#[derive(Default)]
struct ButtonData {
    text: String,
    icon: String,
    selected: bool,
    active: bool,
    bordered: bool,
    left_align: bool,
    accent: bool,
    danger: bool,
    font_size: Option<i32>,
    shortcut: Option<crate::keys::Keys>,
}

/// Omarchy `Button`: optional Nerd glyph + text, transparent at rest,
/// hover/pressed/selected fills from `[controls]`, bold when selected.
#[derive(Clone)]
pub struct Button {
    inner: Widget,
    data: Rc<RefCell<ButtonData>>,
    click: Cb<()>,
}
fltk::widget_extends!(Button, Widget, inner);

impl Button {
    pub fn new(text: &str) -> Self {
        let mut inner = Widget::default();
        inner.set_frame(FrameType::NoBox);
        let data = Rc::new(RefCell::new(ButtonData { text: text.into(), ..Default::default() }));
        let st = Rc::new(RefCell::new(Interaction::default()));
        let click: Cb<()> = cb();
        let c2 = click.clone();
        let d3 = data.clone();
        track_with_shortcut(&mut inner, st.clone(), true, Rc::new(move || fire(&c2, ())), Rc::new(move || d3.borrow().shortcut));
        let d = data.clone();
        let mut fade = Anim::new();
        inner.draw(move |w| {
            let d = d.borrow();
            theme::with(|t| {
                let mut s = interaction(w, &st);
                s.selected |= d.selected || d.active;
                let under = under();
                let base = if d.accent { Some(t.palette.accent) } else { None };
                let look = fade.get(paint::control_look(t, s, under, base, d.bordered || d.accent), 120, Ease::Linear, w);
                paint::draw_look(w.x(), w.y(), w.w(), w.h(), t.radius, look, under);
                let mut fg = if d.accent {
                    if t.palette.accent.luminance() > 0.55 { t.palette.background } else { t.palette.foreground }
                } else if d.danger {
                    t.palette.urgent
                } else {
                    t.palette.foreground
                };
                if s.disabled {
                    fg = fg.darker(2.0);
                }
                let fs = d.font_size.unwrap_or(t.font.body);
                let gap = t.spacing.control_gap;
                let iw = if d.icon.is_empty() { 0 } else { paint::measure(&d.icon, t.font.icon, false) };
                let tw = if d.text.is_empty() { 0 } else { paint::measure(&d.text, fs, s.selected) };
                let row = iw + tw + if iw > 0 && tw > 0 { gap } else { 0 };
                let pad = t.spacing.control_padding_x;
                let mut x = if d.left_align { w.x() + pad + 1 } else { w.x() + (w.w() - row) / 2 };
                if iw > 0 {
                    paint::glyph(&d.icon, x, w.y(), iw, w.h(), t.font.icon, fg);
                    x += iw + gap;
                }
                if tw > 0 {
                    let avail = w.x() + w.w() - pad - x;
                    paint::text_elided(&d.text, x, w.y(), avail.max(tw.min(avail)), w.h(), fs, s.selected, fg, Align::Left);
                }
            });
        });
        let mut b = Self { inner, data, click };
        let (w, h) = b.preferred_size();
        b.inner.set_size(w, h);
        b
    }

    /// Icon-only or icon + text button: `Button::icon(glyph::PLUS, "Add")`.
    pub fn icon(glyph: &str, text: &str) -> Self {
        let b = Self::new(text);
        b.data.borrow_mut().icon = glyph.into();
        b.resized()
    }

    fn resized(mut self) -> Self {
        let (w, h) = self.preferred_size();
        self.inner.set_size(w, h);
        self
    }

    pub fn on_click(self, f: impl FnMut() + 'static) -> Self {
        let mut f = f;
        *self.click.borrow_mut() = Some(Box::new(move |_| f()));
        self
    }

    /// Filled with the theme accent (primary action).
    pub fn primary(self) -> Self {
        self.data.borrow_mut().accent = true;
        self
    }

    /// Text in the urgent color (destructive action).
    pub fn danger(self) -> Self {
        self.data.borrow_mut().danger = true;
        self
    }

    /// Normal-state border (Omarchy `bordered: true`).
    pub fn bordered(self) -> Self {
        self.data.borrow_mut().bordered = true;
        self
    }

    pub fn left_aligned(self) -> Self {
        self.data.borrow_mut().left_align = true;
        self
    }

    pub fn tooltip(mut self, text: &str) -> Self {
        self.inner.set_tooltip(text);
        self
    }

    pub fn font_size(self, px: i32) -> Self {
        self.data.borrow_mut().font_size = Some(px);
        self.resized()
    }

    /// Keyboard shortcut such as "Ctrl+S" (active while the window has focus).
    pub fn shortcut(self, spec: &str) -> Self {
        let keys = crate::keys::parse(spec).unwrap_or_else(|e| panic!("{e}"));
        self.data.borrow_mut().shortcut = Some(keys);
        self
    }

    pub fn set_selected(&mut self, on: bool) {
        self.data.borrow_mut().selected = on;
        self.inner.redraw();
    }

    pub fn set_text(&mut self, text: &str) {
        self.data.borrow_mut().text = text.into();
        self.inner.redraw();
    }

    pub fn text(&self) -> String {
        self.data.borrow().text.clone()
    }

    /// Keeps the label in sync with a State.
    pub fn bind_text(self, s: &State<String>) -> Self {
        let mut me = self.clone();
        crate::state::bind_widget(&self.inner, s, true, move |v| me.set_text(v));
        self
    }

    /// Enables the button while the State is true.
    pub fn bind_enabled(self, s: &State<bool>) -> Self {
        let mut me = self.inner.clone();
        crate::state::bind_widget(&self.inner, s, true, move |on| if *on { me.activate() } else { me.deactivate() });
        self
    }

    /// Runs the click callback (programmatic click, for tests and scripts).
    pub fn click(&self) {
        fire(&self.click, ());
    }

    /// Omarchy implicit size: content + 2 × control padding + 1 px border.
    pub fn preferred_size(&self) -> (i32, i32) {
        let d = self.data.borrow();
        theme::with(|t| {
            let fs = d.font_size.unwrap_or(t.font.body);
            let iw = if d.icon.is_empty() { 0 } else { measure_offscreen(&d.icon, t.font.icon) };
            let tw = if d.text.is_empty() { 0 } else { measure_offscreen(&d.text, fs) };
            let row = iw + tw + if iw > 0 && tw > 0 { t.spacing.control_gap } else { 0 };
            let h = t.spacing.control_height;
            ((row + 2 * t.spacing.control_padding_x + 2).max(h), h)
        })
    }
}

/// Text width usable outside draw callbacks (fltk needs an open display).
pub fn measure_offscreen(s: &str, size: i32) -> i32 {
    draw::set_font(Font::Helvetica, size);
    draw::width(s).ceil() as i32
}

// ============================================================ ActionButton

/// Omarchy `PanelActionButton`: square icon button, transparent until
/// hovered; pass `.destructive()` to hover in the urgent color.
#[derive(Clone)]
pub struct ActionButton {
    inner: Widget,
    click: Cb<()>,
}
fltk::widget_extends!(ActionButton, Widget, inner);

impl ActionButton {
    pub fn new(glyph: &str, tooltip: &str) -> Self {
        Self::build(glyph, tooltip, false)
    }

    pub fn destructive(glyph: &str, tooltip: &str) -> Self {
        Self::build(glyph, tooltip, true)
    }

    fn build(glyph: &str, tooltip: &str, urgent: bool) -> Self {
        let mut inner = Widget::default();
        inner.set_frame(FrameType::NoBox);
        if !tooltip.is_empty() {
            inner.set_tooltip(tooltip);
        }
        let st = Rc::new(RefCell::new(Interaction::default()));
        let click: Cb<()> = cb();
        let c2 = click.clone();
        track(&mut inner, st.clone(), true, Rc::new(move || fire(&c2, ())));
        let g = glyph.to_string();
        let mut fade = Anim::new();
        inner.draw(move |w| {
            theme::with(|t| {
                let s = interaction(w, &st);
                let fg = t.palette.foreground;
                let hover_color = if urgent { t.palette.urgent } else { fg };
                let under = under();
                let hot = (s.hover || s.pressed) && !s.disabled;
                let fill = if s.focus {
                    Some(hover_color.over(under, t.controls.focus.fill_alpha))
                } else if hot {
                    Some(hover_color.over(under, if s.pressed { t.controls.pressed_fill_alpha } else { t.controls.hover.fill_alpha }))
                } else {
                    None
                };
                let border = s.focus.then(|| (t.border(&t.controls.focus, under), t.controls.focus.border_width));
                let f = fill.unwrap_or(under);
                let target = match border { Some((c, bw)) => paint::Look { fill: f, border: c, border_width: bw }, None => paint::Look { fill: f, border: f, border_width: 0 } };
                let look = fade.get(target, 60, Ease::Linear, w);
                paint::draw_look(w.x(), w.y(), w.w(), w.h(), t.radius, look, under);
                let color = if s.disabled { fg.darker(2.0) } else if hot { hover_color } else { fg };
                paint::glyph(&g, w.x(), w.y(), w.w(), w.h(), t.font.icon, color);
            });
        });
        let size = theme::with(|t| (22.0 * t.font.base as f64 / 12.0).round() as i32).max(22);
        inner.set_size(size, size);
        Self { inner, click }
    }

    pub fn on_click(self, f: impl FnMut() + 'static) -> Self {
        let mut f = f;
        *self.click.borrow_mut() = Some(Box::new(move |_| f()));
        self
    }

    pub fn click(&self) {
        fire(&self.click, ());
    }
}

// ============================================================= ButtonGroup

/// Omarchy `ButtonGroup`: a row of bordered chips; exactly one selected.
/// Keyboard: Left/Right (h/l) move, Enter/Space choose.
#[derive(Clone)]
pub struct ButtonGroup {
    inner: fltk::group::Flex,
    buttons: Rc<RefCell<Vec<Button>>>,
    value: State<usize>,
}
fltk::widget_extends!(ButtonGroup, fltk::group::Flex, inner);

impl ButtonGroup {
    /// `ButtonGroup::new(&["Day", "Week", "Month"], &state)`; the State holds the selected index.
    pub fn new(options: &[&str], value: &State<usize>) -> Self {
        let mut inner = fltk::group::Flex::default().row();
        inner.set_spacing(theme::with(|t| t.spacing.md));
        let buttons: Rc<RefCell<Vec<Button>>> = Rc::default();
        let mut total = 0;
        for (i, label) in options.iter().enumerate() {
            let v = value.clone();
            let b = Button::new(label).bordered().on_click(move || v.set(i));
            let (w, _) = b.preferred_size();
            inner.fixed(&*b, w);
            total += w;
            buttons.borrow_mut().push(b);
        }
        inner.end();
        let h = theme::with(|t| t.spacing.control_height);
        let gap = inner.spacing() * (options.len().max(1) as i32 - 1);
        inner.resize(inner.x(), inner.y(), total + gap, h);
        // One Tab stop (the selected button); Left/Right move the selection.
        let bs = buttons.clone();
        crate::state::bind_widget(&inner, value, true, move |sel| {
            for (i, b) in bs.borrow_mut().iter_mut().enumerate() {
                b.set_selected(i == *sel);
                if i == *sel { b.set_visible_focus() } else { b.clear_visible_focus() }
            }
        });
        let (bs, v, n) = (buttons.clone(), value.clone(), options.len());
        crate::focus::scope(&inner, move |k| {
            let cur = v.get();
            let next = match k {
                Key::Left if cur > 0 => cur - 1,
                Key::Right if cur + 1 < n => cur + 1,
                Key::Left | Key::Right => return true,
                _ => return false,
            };
            v.set(next);
            let b = bs.borrow().get(next).cloned();
            if let Some(mut b) = b { let _ = b.take_focus(); }
            true
        });
        Self { inner, buttons, value: value.clone() }
    }

    pub fn selected(&self) -> usize {
        self.value.get()
    }

    pub fn button(&self, i: usize) -> Option<Button> {
        self.buttons.borrow().get(i).cloned()
    }
}

// ============================================================ ToggleSwitch

/// Omarchy `ToggleSwitch`: 42×22 track, 16 px knob, selected fill when on.
#[derive(Clone)]
pub struct ToggleSwitch {
    inner: Widget,
    value: State<bool>,
}
fltk::widget_extends!(ToggleSwitch, Widget, inner);

/// `k`: 0 = off, 1 = on (fractions while the knob moves).
pub(crate) fn draw_switch(t: &theme::Theme, x: i32, y: i32, k: f64, hot: bool, under: Rgb) {
    let th = t.spacing.control_height.max(28);
    let track_h = 22.max((th as f64 * 0.55).round() as i32);
    let track_w = (track_h as f64 * 1.9).round() as i32;
    let knob = 6.max((track_h as f64 * 0.72).round() as i32);
    let inset = 1.max((track_h - knob) / 2);
    let rounded = t.radius > 0;
    let r = if rounded { track_h / 2 } else { 0 };
    let c = &t.controls;
    let look = |on: bool| {
        let (fill, border) = if on {
            (t.fill(&c.selected, under), (c.selected.border_width > 0).then(|| (t.border(&c.selected, under), c.selected.border_width)))
        } else {
            (t.fill(&c.normal, under), Some((t.border(&c.normal, under), c.normal.border_width)))
        };
        let fill = if hot { c.hover.color.over(fill, c.hover.fill_alpha) } else { fill };
        match border { Some((b, bw)) => paint::Look { fill, border: b, border_width: bw }, None => paint::Look { fill, border: fill, border_width: 0 } }
    };
    let k = k.clamp(0.0, 1.0);
    let l = paint::Look::lerp(look(false), look(true), k);
    paint::surface(x, y, track_w, track_h, r, Some(l.fill), (l.border_width > 0).then_some((l.border, l.border_width)));
    let (x_off, x_on) = (x + inset, x + track_w - knob - inset);
    let kx = (x_off as f64 + (x_on - x_off) as f64 * k).round() as i32;
    let kc = Rgb::lerp(t.palette.foreground.darker(1.25), c.selected.color, k);
    paint::fill(kx, y + (track_h - knob) / 2, knob, knob, if rounded { knob / 2 } else { 0 }, kc);
}

pub(crate) fn switch_size() -> (i32, i32) {
    theme::with(|t| {
        let th = t.spacing.control_height.max(28);
        let h = 22.max((th as f64 * 0.55).round() as i32);
        ((h as f64 * 1.9).round() as i32, h)
    })
}

impl ToggleSwitch {
    pub fn new(value: &State<bool>) -> Self {
        let mut inner = Widget::default();
        inner.set_frame(FrameType::NoBox);
        let st = Rc::new(RefCell::new(Interaction::default()));
        let v = value.clone();
        track(&mut inner, st.clone(), true, Rc::new(move || v.update(|b| *b = !*b)));
        let v = value.clone();
        let mut knob = Anim::new();
        inner.draw(move |w| {
            let s = interaction(w, &st);
            let k = knob.get(if v.get() { 1.0 } else { 0.0 }, 120, Ease::OutCubic, w);
            theme::with(|t| {
                let (tw, th) = switch_size();
                let x = w.x() + (w.w() - tw) / 2;
                let y = w.y() + (w.h() - th) / 2;
                if s.hover || s.focus {
                    let pad = t.spacing.md;
                    paint::surface(x - pad, y - pad, tw + 2 * pad, th + 2 * pad, t.radius, None,
                                   Some((t.border(&t.controls.hover, under()), t.controls.hover.border_width.max(1))));
                }
                draw_switch(t, x, y, k, false, under());
            });
        });
        let (tw, th) = switch_size();
        let pad = theme::with(|t| t.spacing.md);
        inner.set_size(tw + 2 * pad, th + 2 * pad);
        let mut w2 = inner.clone();
        crate::state::bind_widget(&inner, value, false, move |_| w2.redraw());
        Self { inner, value: value.clone() }
    }

    pub fn is_on(&self) -> bool {
        self.value.get()
    }
}

// ================================================================== Toggle

/// Omarchy `Toggle`: a full-width row with bold title, optional
/// description and a switch on the right. The whole row toggles.
#[derive(Clone)]
pub struct Toggle {
    inner: Widget,
    value: State<bool>,
}
fltk::widget_extends!(Toggle, Widget, inner);

impl Toggle {
    pub fn new(title: &str, description: &str, value: &State<bool>) -> Self {
        let mut inner = Widget::default();
        inner.set_frame(FrameType::NoBox);
        let st = Rc::new(RefCell::new(Interaction::default()));
        let v = value.clone();
        track(&mut inner, st.clone(), true, Rc::new(move || v.update(|b| *b = !*b)));
        let (title, desc) = (title.to_string(), description.to_string());
        let v = value.clone();
        let (mut fade, mut knob) = (Anim::new(), Anim::new());
        inner.draw(move |w| {
            let s = interaction(w, &st);
            let k = knob.get(if v.get() { 1.0 } else { 0.0 }, 120, Ease::OutCubic, w);
            theme::with(|t| {
                let under = under();
                let target = paint::control_look(t, Interaction { selected: false, ..s }, under, Some(t.fill(&t.controls.normal, under)), true);
                let look = fade.get(target, 100, Ease::Linear, w);
                paint::draw_look(w.x(), w.y(), w.w(), w.h(), t.radius, look, under);
                let pad = t.spacing.row_padding_x + 1;
                let (sw, sh) = switch_size();
                let text_w = w.w() - 2 * pad - sw - t.spacing.xxl;
                let fg = t.palette.foreground;
                let th = t.font.subtitle + 4;
                let dh = if desc.is_empty() { 0 } else { t.font.caption + 4 };
                let block = th + if dh > 0 { t.spacing.xs + dh } else { 0 };
                let top = w.y() + (w.h() - block) / 2;
                paint::text_elided(&title, w.x() + pad, top, text_w, th, t.font.subtitle, true, fg, Align::Left);
                if dh > 0 {
                    paint::text_elided(&desc, w.x() + pad, top + th + t.spacing.xs, text_w, dh, t.font.caption, false, fg.darker(1.5), Align::Left);
                }
                draw_switch(t, w.x() + w.w() - pad - sw, w.y() + (w.h() - sh) / 2, k, false, under);
            });
        });
        let h = theme::with(|t| 54.max(t.font.subtitle + t.font.caption + 8 + t.spacing.huge));
        inner.set_size(theme::with(|t| (240.0 * t.font.base as f64 / 12.0) as i32), h);
        let mut w2 = inner.clone();
        crate::state::bind_widget(&inner, value, false, move |_| w2.redraw());
        Self { inner, value: value.clone() }
    }

    pub fn is_on(&self) -> bool {
        self.value.get()
    }

    /// Programmatic toggle (same as a click).
    pub fn toggle(&self) {
        self.value.update(|b| *b = !*b);
    }
}

// ================================================================== Slider

/// Omarchy `PanelSlider`: 4 px track, foreground fill, 14 px round knob
/// with a 2 px ring, scales 1.15× while hot. Mouse drag and wheel.
#[derive(Clone)]
pub struct Slider {
    inner: Widget,
    value: State<f64>,
}
fltk::widget_extends!(Slider, Widget, inner);

impl Slider {
    /// `Slider::new(&volume, 0.0, 1.0, 0.05)`
    pub fn new(value: &State<f64>, min: f64, max: f64, step: f64) -> Self {
        let mut inner = Widget::default();
        inner.set_frame(FrameType::NoBox);
        inner.set_visible_focus();
        let hot = Rc::new(RefCell::new(false));
        let dragging = Rc::new(Cell::new(false));
        let range = (max - min).max(f64::EPSILON);
        let v = value.clone();
        let h2 = hot.clone();
        let dr = dragging.clone();
        inner.handle(move |w, ev| {
            let set_from_x = |w: &Widget| {
                let k = theme::with(|t| 14.max((t.spacing.control_height as f64 * 0.38).round() as i32));
                let p = ((app::event_x() - w.x() - k / 2) as f64 / (w.w() - k).max(1) as f64).clamp(0.0, 1.0);
                let mut nv = min + p * range;
                if step > 0.0 {
                    nv = (nv / step).round() * step;
                }
                v.set(nv.clamp(min, max));
            };
            match ev {
                Event::Enter => { *h2.borrow_mut() = true; w.redraw(); true }
                Event::Leave => { *h2.borrow_mut() = false; w.redraw(); true }
                Event::Push => { let _ = w.take_focus(); dr.set(true); set_from_x(w); true }
                Event::Drag => { set_from_x(w); true }
                Event::Released => { dr.set(false); true }
                Event::MouseWheel => {
                    let dir = match app::event_dy() { app::MouseWheel::Up => 1.0, app::MouseWheel::Down => -1.0, _ => 0.0 };
                    v.update(|x| *x = (*x + dir * step.max(range / 100.0)).clamp(min, max));
                    true
                }
                Event::KeyDown => {
                    let d = match app::event_key() { Key::Right | Key::Up => 1.0, Key::Left | Key::Down => -1.0, _ => return false };
                    v.update(|x| *x = (*x + d * step.max(range / 100.0)).clamp(min, max));
                    true
                }
                Event::Focus | Event::Unfocus => { w.redraw(); true }
                _ => false,
            }
        });
        let v = value.clone();
        let (mut scale, mut pos, dragging) = (Anim::new(), Anim::new(), dragging.clone());
        inner.draw(move |w| {
            theme::with(|t| {
                let under = under();
                let fg = t.palette.foreground;
                let track_h = 4.max((t.spacing.control_height as f64 * 0.11).round() as i32);
                let base = 14.max((t.spacing.control_height as f64 * 0.38).round() as i32);
                let grow = scale.get(if *hot.borrow() { 1.15 } else { 1.0 }, 110, Ease::OutCubic, w);
                let knob = (base as f64 * grow).round() as i32;
                let target = ((v.get() - min) / range).clamp(0.0, 1.0);
                // PanelSlider: position eases (140 ms OutCubic) except while dragging.
                let p = if dragging.get() { pos.jump(target) } else { pos.get(target, 140, Ease::OutCubic, w) };
                let ty = w.y() + (w.h() - track_h) / 2;
                let r = track_h / 2;
                paint::fill(w.x(), ty, w.w(), track_h, r, t.fill(&t.controls.selected, under));
                paint::fill(w.x(), ty, ((w.w() as f64) * p).round() as i32, track_h, r, if w.active_r() { fg } else { fg.darker(2.0) });
                let kx = (w.x() as f64 + w.w() as f64 * p - knob as f64 / 2.0).round() as i32;
                let kx = kx.clamp(w.x(), w.x() + w.w() - knob);
                let ky = w.y() + (w.h() - knob) / 2;
                draw::set_draw_color(under.fl());
                draw::draw_pie(kx - 2, ky - 2, knob + 4, knob + 4, 0.0, 360.0);
                draw::set_draw_color(fg.fl());
                draw::draw_pie(kx, ky, knob, knob, 0.0, 360.0);
                if crate::focus::show_ring(w) {
                    paint::stroke(w.x(), w.y(), w.w(), w.h(), t.radius, 1, t.border(&t.controls.focus, under));
                }
            });
        });
        let h = theme::with(|t| 22.max(14 + 6).max(t.spacing.control_height - 6));
        inner.set_size(theme::with(|t| (200.0 * t.font.base as f64 / 12.0) as i32), h);
        let mut w2 = inner.clone();
        crate::state::bind_widget(&inner, value, false, move |_| w2.redraw());
        Self { inner, value: value.clone() }
    }

    pub fn value(&self) -> f64 {
        self.value.get()
    }
}

// ================================================================ Progress

/// Read-only progress bar in the PanelSlider track style (no knob).
#[derive(Clone)]
pub struct Progress {
    inner: Widget,
}
fltk::widget_extends!(Progress, Widget, inner);

impl Progress {
    /// `value` in 0..=1.
    pub fn new(value: &State<f64>) -> Self {
        let mut inner = Widget::default();
        inner.set_frame(FrameType::NoBox);
        let v = value.clone();
        inner.draw(move |w| {
            theme::with(|t| {
                let track_h = 4.max((t.spacing.control_height as f64 * 0.11).round() as i32);
                let ty = w.y() + (w.h() - track_h) / 2;
                paint::fill(w.x(), ty, w.w(), track_h, track_h / 2, t.fill(&t.controls.selected, under()));
                let p = v.get().clamp(0.0, 1.0);
                paint::fill(w.x(), ty, (w.w() as f64 * p).round() as i32, track_h, track_h / 2, t.palette.accent);
            });
        });
        inner.set_size(200, theme::with(|t| t.spacing.lg * 2));
        let mut w2 = inner.clone();
        crate::state::bind_widget(&inner, value, false, move |_| w2.redraw());
        Self { inner }
    }
}

// =================================================================== Label

/// Text role from the Omarchy type scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextStyle {
    Caption,
    BodySmall,
    Body,
    Subtitle,
    Title,
    Heading,
    Display,
    DisplayLarge,
}

impl TextStyle {
    pub fn px(self, t: &theme::Theme) -> i32 {
        let f = &t.font;
        match self {
            TextStyle::Caption => f.caption,
            TextStyle::BodySmall => f.body_small,
            TextStyle::Body => f.body,
            TextStyle::Subtitle => f.subtitle,
            TextStyle::Title => f.title,
            TextStyle::Heading => f.heading,
            TextStyle::Display => f.display,
            TextStyle::DisplayLarge => f.display_large,
        }
    }
}

/// Color role for text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Normal,
    /// `darker(fg, 1.4)` — secondary text, section headers, meta lines.
    Dim,
    /// `darker(fg, 1.5)` — descriptions.
    Faint,
    Accent,
    Urgent,
}

fn tone_color(t: &theme::Theme, tone: Tone) -> Rgb {
    let fg = t.palette.foreground;
    match tone {
        Tone::Normal => fg,
        Tone::Dim => fg.darker(1.4),
        Tone::Faint => fg.darker(1.5),
        Tone::Accent => t.palette.accent,
        Tone::Urgent => t.palette.urgent,
    }
}

#[derive(Clone)]
struct LabelData {
    text: String,
    style: TextStyle,
    bold: bool,
    tone: Tone,
    align: Align,
    uppercase: bool,
}

/// Themed text using the Omarchy type scale and tones; elides on overflow.
#[derive(Clone)]
pub struct Label {
    inner: Widget,
    data: Rc<RefCell<LabelData>>,
}
fltk::widget_extends!(Label, Widget, inner);

impl Label {
    pub fn new(text: &str) -> Self {
        Self::styled(text, TextStyle::Body)
    }

    pub fn styled(text: &str, style: TextStyle) -> Self {
        let mut inner = Widget::default();
        inner.set_frame(FrameType::NoBox);
        let data = Rc::new(RefCell::new(LabelData { text: text.into(), style, bold: false, tone: Tone::Normal, align: Align::Left, uppercase: false }));
        let d = data.clone();
        inner.draw(move |w| {
            let d = d.borrow();
            theme::with(|t| {
                let s = if d.uppercase { d.text.to_uppercase() } else { d.text.clone() };
                let mut c = tone_color(t, d.tone);
                if !w.active_r() { c = c.darker(2.0); }
                paint::text_elided(&s, w.x(), w.y(), w.w(), w.h(), d.style.px(t), d.bold, c, d.align);
            });
        });
        let mut l = Self { inner, data };
        l.fit();
        l
    }

    fn fit(&mut self) {
        let d = self.data.borrow().clone();
        let (w, h) = theme::with(|t| {
            let px = d.style.px(t);
            (measure_offscreen(&d.text, px) + 2, (px as f64 * 1.5).round() as i32)
        });
        self.inner.set_size(w, h);
    }

    pub fn bold(self) -> Self { self.data.borrow_mut().bold = true; self }
    pub fn tone(self, tone: Tone) -> Self { self.data.borrow_mut().tone = tone; self }
    pub fn center(self) -> Self { self.data.borrow_mut().align = Align::Center; self }
    pub fn right(self) -> Self { self.data.borrow_mut().align = Align::Right; self }

    pub fn set_text(&mut self, text: &str) {
        self.data.borrow_mut().text = text.into();
        let d = self.data.borrow().clone();
        let natural = theme::with(|t| {
            let px = d.style.px(t);
            let s = if d.uppercase { d.text.to_uppercase() } else { d.text.clone() };
            (measure_offscreen(&s, px) + 2, (px as f64 * 1.5).round() as i32)
        });
        if !crate::layout::refit(&self.inner, natural) {
            // Not in a flex yet (bound before `fit`): take the natural size
            // so the later `fit` keeps it.
            self.inner.set_size(natural.0, natural.1);
        }
        self.inner.redraw();
    }

    pub fn text(&self) -> String {
        self.data.borrow().text.clone()
    }

    pub fn bind(self, s: &State<String>) -> Self {
        let mut me = self.clone();
        crate::state::bind_widget(&self.inner, s, true, move |v| me.set_text(v));
        self
    }

    /// Omarchy `PanelSectionHeader`: caption, bold, dim, uppercase.
    pub fn section(text: &str) -> Self {
        let l = Self::styled(text, TextStyle::Caption).bold().tone(Tone::Dim);
        l.data.borrow_mut().uppercase = true;
        let mut l = l;
        l.fit();
        l
    }
}

// =============================================================== Separator

/// Omarchy `PanelSeparator`: 1 px line, `rgba(fg, 0.12)`.
#[derive(Clone)]
pub struct Separator {
    inner: Widget,
}
fltk::widget_extends!(Separator, Widget, inner);

impl Separator {
    pub fn new() -> Self {
        let mut inner = Widget::default();
        inner.set_frame(FrameType::NoBox);
        inner.draw(|w| theme::with(|t| paint::hairline(w.x(), w.y() + w.h() / 2, w.w(), t.palette.foreground, under(), 0.12)));
        inner.set_size(10, 1);
        Self { inner }
    }
}

impl Default for Separator {
    fn default() -> Self {
        Self::new()
    }
}

// ==================================================================== Hero

/// Omarchy `PanelHero`: big glyph, bold title with optional detail pill,
/// and an uppercase dim meta line.
#[derive(Clone)]
pub struct Hero {
    inner: Widget,
    data: Rc<RefCell<(String, String, String, String)>>,
}
fltk::widget_extends!(Hero, Widget, inner);

impl Hero {
    pub fn new(icon: &str, title: &str, meta: &str) -> Self {
        let mut inner = Widget::default();
        inner.set_frame(FrameType::NoBox);
        let data = Rc::new(RefCell::new((icon.to_string(), title.to_string(), meta.to_string(), String::new())));
        let d = data.clone();
        inner.draw(move |w| {
            let (icon, title, meta, detail) = d.borrow().clone();
            theme::with(|t| {
                let fg = t.palette.foreground;
                let iw = paint::measure(&icon, t.font.display, false).max(t.font.display);
                paint::glyph(&icon, w.x(), w.y(), iw, w.h(), t.font.display, fg);
                let x = w.x() + iw + t.spacing.xxxl;
                let avail = w.x() + w.w() - x;
                let th = t.font.title + 6;
                let mh = t.font.caption + 4;
                let top = w.y() + (w.h() - th - mh - 2) / 2;
                let mut title_w = avail;
                if !detail.is_empty() {
                    let pw = paint::measure(&detail, t.font.body, true) + t.spacing.xl;
                    let ph = t.font.body + t.spacing.sm + 4;
                    let px = w.x() + w.w() - pw;
                    paint::surface(px, top + (th - ph) / 2, pw, ph, t.radius, None,
                                   Some((t.border(&t.controls.normal, under()), t.controls.normal.border_width)));
                    paint::text(&detail, px, top + (th - ph) / 2, pw, ph, t.font.body, true, fg.darker(1.4), Align::Center);
                    title_w -= pw + t.spacing.lg;
                }
                paint::text_elided(&title, x, top, title_w, th, t.font.title, true, fg, Align::Left);
                paint::text_elided(&meta.to_uppercase(), x, top + th + 2, avail, mh, t.font.caption, true, fg.darker(1.4), Align::Left);
            });
        });
        inner.set_size(300, theme::with(|t| t.font.display + t.spacing.huge * 2));
        Self { inner, data }
    }

    /// Small bordered pill to the right of the title (e.g. "42%").
    pub fn detail(self, text: &str) -> Self {
        self.data.borrow_mut().3 = text.into();
        self
    }

    pub fn set_title(&mut self, s: &str) { self.data.borrow_mut().1 = s.into(); self.inner.redraw(); }
    pub fn set_meta(&mut self, s: &str) { self.data.borrow_mut().2 = s.into(); self.inner.redraw(); }
    pub fn set_detail(&mut self, s: &str) { self.data.borrow_mut().3 = s.into(); self.inner.redraw(); }
}

// ============================================================ TextField

/// Omarchy `TextField`: control chrome (normal/hover/focus), body font,
/// placeholder in `darker(fg, 1.6)`, selection `rgba(fg, .35)`.
#[derive(Clone)]
pub struct TextField {
    inner: fltk::group::Group,
    input: fltk::input::Input,
    change: Cb<String>,
    submit: Cb<String>,
    placeholder: Rc<RefCell<String>>,
}
fltk::widget_extends!(TextField, fltk::group::Group, inner);

impl TextField {
    pub fn new() -> Self {
        Self::build(false)
    }

    pub fn password() -> Self {
        Self::build(true)
    }

    fn build(secret: bool) -> Self {
        let (h, padx) = theme::with(|t| (t.font.body + 4 + 2 * t.spacing.input_padding_y + 2, t.spacing.control_padding_x));
        let mut inner = fltk::group::Group::default().with_size(200, h);
        inner.set_frame(FrameType::NoBox);
        let mut input = fltk::input::Input::default().with_size(200 - 2 * padx - 2, h - 2);
        if secret {
            input.set_type(fltk::input::InputType::Secret);
        }
        input.set_pos(padx + 1, 1);
        // Its own flat background (set to the field fill on every draw): when
        // only the text changes, fltk redraws the input alone and must clear it.
        input.set_frame(FrameType::FlatBox);
        inner.end();
        inner.resizable(&input);
        let hover = Rc::new(RefCell::new(false));
        let placeholder: Rc<RefCell<String>> = Rc::default();
        let change: Cb<String> = cb();
        let submit: Cb<String> = cb();

        let style_input = |input: &mut fltk::input::Input, fill: Rgb| {
            theme::with(|t| {
                input.set_color(fill.fl());
                input.set_text_color(t.palette.foreground.fl());
                input.set_text_size(t.font.body);
                input.set_text_font(Font::Helvetica);
                input.set_cursor_color(t.palette.foreground.fl());
                input.set_selection_color(t.palette.foreground.over(fill, t.controls.selection_fill_alpha).fl());
            });
        };

        let (inp, hv, ph) = (input.clone(), hover.clone(), placeholder.clone());
        inner.draw(move |g| {
            let mut inp = inp.clone();
            let focused = app::focus().is_some_and(|f| f.as_widget_ptr() == inp.as_widget_ptr());
            theme::with(|t| {
                let under = under();
                let s = Interaction { hover: *hv.borrow(), focus: focused, ..Default::default() };
                let st = if focused { &t.controls.focus } else if s.hover { &t.controls.hover } else { &t.controls.normal };
                let fill = t.fill(st, under);
                paint::surface(g.x(), g.y(), g.w(), g.h(), t.radius, Some(fill), Some((t.border(st, under), st.border_width)));
                style_input(&mut inp, fill);
            });
            g.draw_children();
            if inp.value().is_empty() && !ph.borrow().is_empty() {
                theme::with(|t| {
                    let x = inp.x() + if focused { 3 } else { 0 };
                    paint::text_elided(&ph.borrow(), x, inp.y(), inp.w() - 3, inp.h(), t.font.body, false, t.palette.foreground.darker(1.6), Align::Left);
                });
            }
        });
        // cfltk runs FLTK's own draw before this closure by default: it would
        // draw damaged children first and this background would then cover them.
        inner.super_draw(false);
        let hv = hover.clone();
        inner.handle(move |g, ev| match ev {
            Event::Enter => { *hv.borrow_mut() = true; g.redraw(); false }
            Event::Leave => { *hv.borrow_mut() = false; g.redraw(); false }
            _ => false,
        });
        let (c2, s2) = (change.clone(), submit.clone());
        input.set_trigger(CallbackTrigger::Changed | CallbackTrigger::EnterKey);
        let mut parent = inner.clone();
        input.set_callback(move |i| {
            parent.redraw();
            if app::event() == Event::KeyDown && matches!(app::event_key(), Key::Enter | Key::KPEnter) {
                fire(&s2, i.value());
            } else {
                fire(&c2, i.value());
            }
        });
        let mut parent = inner.clone();
        input.handle(move |_, ev| {
            if matches!(ev, Event::Focus | Event::Unfocus) {
                parent.redraw();
            }
            false
        });
        Self { inner, input, change, submit, placeholder }
    }

    pub fn placeholder(self, text: &str) -> Self {
        *self.placeholder.borrow_mut() = text.into();
        self
    }

    pub fn on_change(self, f: impl FnMut(String) + 'static) -> Self {
        *self.change.borrow_mut() = Some(Box::new(f));
        self
    }

    /// Enter key.
    pub fn on_submit(self, f: impl FnMut(String) + 'static) -> Self {
        *self.submit.borrow_mut() = Some(Box::new(f));
        self
    }

    /// Two-way binding: edits write the State, State changes update the text.
    pub fn bind(self, s: &State<String>) -> Self {
        let mut input = self.input.clone();
        let mut g = self.inner.clone();
        crate::state::bind_widget(&self.inner, s, true, move |v| {
            if input.value() != *v {
                input.set_value(v);
                g.redraw();
            }
        });
        let s2 = s.clone();
        let prev = self.change.borrow_mut().take();
        let mut prev = prev;
        *self.change.borrow_mut() = Some(Box::new(move |v: String| {
            s2.set(v.clone());
            if let Some(p) = prev.as_mut() { p(v); }
        }));
        self
    }

    pub fn value(&self) -> String {
        self.input.value()
    }

    /// Sets the text and runs on_change, as if the user typed it.
    pub fn type_text(&mut self, text: &str) {
        self.input.set_value(text);
        fire(&self.change, text.to_string());
        self.inner.redraw();
    }

    pub fn submit(&self) {
        fire(&self.submit, self.input.value());
    }

    pub fn input(&self) -> fltk::input::Input {
        self.input.clone()
    }
}

impl Default for TextField {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================ NumberField

/// Omarchy `NumberField`: centered integer field (120 px), Up/Down and the
/// wheel step the value, clamped to `from..=to`.
#[derive(Clone)]
pub struct NumberField {
    field: TextField,
    value: State<i64>,
}

impl std::ops::Deref for NumberField {
    type Target = fltk::group::Group;
    fn deref(&self) -> &Self::Target { &self.field.inner }
}
impl std::ops::DerefMut for NumberField {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.field.inner }
}

impl NumberField {
    pub fn new(value: &State<i64>, from: i64, to: i64, step: i64) -> Self {
        let field = TextField::new();
        let mut input = field.input.clone();
        input.set_type(fltk::input::InputType::Int);
        input.set_align(Align::Center);
        let w = theme::with(|t| t.spacing.number_field_width);
        let mut f2 = field.clone();
        f2.inner.set_size(w, field.inner.h());
        let v = value.clone();
        *field.change.borrow_mut() = Some(Box::new(move |s: String| {
            if let Ok(n) = s.trim().parse::<i64>() {
                v.set(n.clamp(from, to));
            }
        }));
        let v = value.clone();
        input.handle(move |_, ev| {
            let d = match ev {
                Event::KeyDown => match app::event_key() { Key::Up => step, Key::Down => -step, _ => return false },
                Event::MouseWheel => match app::event_dy() { app::MouseWheel::Up => step, app::MouseWheel::Down => -step, _ => return false },
                _ => return false,
            };
            v.update(|n| *n = (*n + d).clamp(from, to));
            true
        });
        let mut inp = input.clone();
        crate::state::bind_widget(&input, value, true, move |n| {
            let s = n.to_string();
            if inp.value() != s { inp.set_value(&s); }
        });
        Self { field, value: value.clone() }
    }

    pub fn value(&self) -> i64 {
        self.value.get()
    }
}

// ===================================================================== Row

/// One row in Omarchy's cursor-surface style (panel device rows, menu rows,
/// list rows): optional glyph, label, optional detail; hover = hover fill,
/// `current` = selected fill + bold label.
#[derive(Clone)]
pub struct Row {
    inner: Widget,
    data: Rc<RefCell<(String, String, String, bool)>>,
    click: Cb<()>,
}
fltk::widget_extends!(Row, Widget, inner);

/// CursorSurface background of a row (hover cursor, or current).
pub(crate) fn row_look(t: &theme::Theme, hot: bool, current: bool, under: Rgb) -> paint::Look {
    let c = &t.controls;
    let (fill, border) = if hot {
        (t.fill(&c.hover, under), Some((t.border(&c.hover, under), c.hover.border_width)))
    } else if current {
        (t.fill(&c.selected, under), (c.selected.border_width > 0).then(|| (t.border(&c.selected, under), c.selected.border_width)))
    } else {
        (under, None)
    };
    match border { Some((b, bw)) => paint::Look { fill, border: b, border_width: bw }, None => paint::Look { fill, border: fill, border_width: 0 } }
}

pub(crate) fn draw_row(t: &theme::Theme, x: i32, y: i32, w: i32, h: i32, icon: &str, label: &str, detail: &str, hot: bool, current: bool, under: Rgb) {
    paint::draw_look(x, y, w, h, t.radius, row_look(t, hot, current, under), under);
    draw_row_content(t, x, y, w, h, icon, label, detail, current);
}

pub(crate) fn draw_row_content(t: &theme::Theme, x: i32, y: i32, w: i32, h: i32, icon: &str, label: &str, detail: &str, current: bool) {
    let fg = t.palette.foreground;
    let inset = t.spacing.md;
    let mut tx = x + inset + t.spacing.sm;
    if !icon.is_empty() {
        let cell = (22.0 * t.font.base as f64 / 12.0).round() as i32;
        paint::glyph(icon, x + inset, y, cell, h, t.font.title, fg);
        tx = x + inset + cell + t.spacing.lg;
    }
    let right = x + w - inset - t.spacing.sm;
    let mut label_w = right - tx;
    if !detail.is_empty() {
        let dw = paint::measure(detail, t.font.caption, false).min(label_w / 2);
        paint::text_elided(detail, right - dw, y, dw, h, t.font.caption, false, fg.darker(1.4), Align::Right);
        label_w -= dw + t.spacing.lg;
    }
    paint::text_elided(label, tx, y, label_w, h, t.font.body, current, fg, Align::Left);
}

impl Row {
    pub fn new(icon: &str, label: &str) -> Self {
        let mut inner = Widget::default();
        inner.set_frame(FrameType::NoBox);
        let data = Rc::new(RefCell::new((icon.to_string(), label.to_string(), String::new(), false)));
        let st = Rc::new(RefCell::new(Interaction::default()));
        let click: Cb<()> = cb();
        let c2 = click.clone();
        track(&mut inner, st.clone(), true, Rc::new(move || fire(&c2, ())));
        let d = data.clone();
        let mut fade = Anim::new();
        inner.draw(move |w| {
            let (icon, label, detail, current) = d.borrow().clone();
            let s = interaction(w, &st);
            theme::with(|t| {
                let under = under();
                let look = fade.get(row_look(t, s.hover || s.focus, current, under), 60, Ease::Linear, w);
                paint::draw_look(w.x(), w.y(), w.w(), w.h(), t.radius, look, under);
                draw_row_content(t, w.x(), w.y(), w.w(), w.h(), &icon, &label, &detail, current);
            });
        });
        inner.set_size(240, theme::with(|t| t.font.body + 4 + t.spacing.xl * 2));
        Self { inner, data, click }
    }

    pub fn detail(self, text: &str) -> Self {
        self.data.borrow_mut().2 = text.into();
        self
    }

    pub fn current(self, on: bool) -> Self {
        self.data.borrow_mut().3 = on;
        self
    }

    pub fn set_current(&mut self, on: bool) {
        self.data.borrow_mut().3 = on;
        self.inner.redraw();
    }

    pub fn on_click(self, f: impl FnMut() + 'static) -> Self {
        let mut f = f;
        *self.click.borrow_mut() = Some(Box::new(move |_| f()));
        self
    }

    pub fn click(&self) {
        fire(&self.click, ());
    }
}

// ================================================================== TabBar

/// Tab strip with the control state tokens: selected tab = selected fill +
/// bold, hover = hover fill, close glyph on hover/selected (if closable).
/// Middle click closes. The selected index lives in a `State<usize>`.
/// Keyboard: anywhere in the window Ctrl+Tab / Ctrl+PageDown go to the next
/// tab and Ctrl+Shift+Tab / Ctrl+PageUp to the previous one; when the bar
/// has focus, Left/Right/Home/End pick a tab and Delete closes it.
#[derive(Clone)]
pub struct TabBar {
    inner: Widget,
    tabs: Rc<RefCell<Vec<String>>>,
    value: State<usize>,
    close: Cb<usize>,
    closable: Rc<Cell<bool>>,
    geo: Rc<TabGeo>,
}
fltk::widget_extends!(TabBar, Widget, inner);

/// Tab strip geometry with horizontal scrolling.
struct TabGeo {
    tabs: Rc<RefCell<Vec<String>>>,
    scroll: Rc<Cell<i32>>,
}

enum Hit {
    Tab(usize, bool),
    Arrow(u8),
    None,
}

impl TabGeo {
    fn arrow_w(&self) -> i32 {
        theme::with(|t| t.spacing.control_height)
    }
    /// Content-relative (x, width) of each tab.
    fn layout(&self) -> Vec<(i32, i32)> {
        theme::with(|t| {
            let pad = t.spacing.control_padding_x;
            let mut x = 0;
            self.tabs.borrow().iter().map(|l| {
                let tw = (measure_offscreen(l, t.font.body) + 2 * pad + t.font.body + t.spacing.md).clamp(80, 220);
                let r = (x, tw);
                x += tw + t.spacing.xxs;
                r
            }).collect()
        })
    }
    fn total(&self) -> i32 {
        self.layout().last().map_or(0, |(x, w)| x + w)
    }
    fn overflow(&self, w: &Widget) -> bool {
        self.total() > w.w()
    }
    /// Visible strip (x, width) on screen.
    fn view(&self, w: &Widget) -> (i32, i32) {
        if self.overflow(w) { let a = self.arrow_w(); (w.x() + a, (w.w() - 2 * a).max(1)) } else { (w.x(), w.w()) }
    }
    fn max_scroll(&self, w: &Widget) -> i32 {
        if self.overflow(w) { (self.total() - self.view(w).1).max(0) } else { 0 }
    }
    fn clamp(&self, w: &Widget) {
        self.scroll.set(self.scroll.get().clamp(0, self.max_scroll(w)));
    }
    /// Screen (x, width) of each tab.
    fn tabs_at(&self, w: &Widget) -> Vec<(i32, i32)> {
        let (vx, _) = self.view(w);
        let sc = if self.overflow(w) { self.scroll.get() } else { 0 };
        self.layout().into_iter().map(|(x, tw)| (vx + x - sc, tw)).collect()
    }
    fn reveal(&self, w: &Widget, i: usize) {
        if !self.overflow(w) {
            self.scroll.set(0);
            return;
        }
        let Some(&(x, tw)) = self.layout().get(i) else { return };
        let vw = self.view(w).1;
        let sc = self.scroll.get();
        if x < sc { self.scroll.set(x) } else if x + tw > sc + vw { self.scroll.set(x + tw - vw) }
        self.clamp(w);
    }
    fn hit(&self, w: &Widget, ex: i32) -> Hit {
        if self.overflow(w) {
            let a = self.arrow_w();
            if ex < w.x() + a { return Hit::Arrow(0); }
            if ex >= w.x() + w.w() - a { return Hit::Arrow(1); }
        }
        let pad = theme::with(|t| t.spacing.control_padding_x + t.font.body);
        match self.tabs_at(w).iter().position(|(x, tw)| ex >= *x && ex < x + tw) {
            Some(i) => { let (x, tw) = self.tabs_at(w)[i]; Hit::Tab(i, ex >= x + tw - pad) }
            None => Hit::None,
        }
    }
}

impl TabBar {
    pub fn new(tabs: &[&str], value: &State<usize>) -> TabBar {
        let mut inner = Widget::default();
        inner.set_frame(FrameType::NoBox);
        inner.set_visible_focus();
        let tabs: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(tabs.iter().map(|s| s.to_string()).collect()));
        let hover = Rc::new(Cell::new(-1i32));
        let closable = Rc::new(Cell::new(false));
        let close: Cb<usize> = cb();
        let scroll = Rc::new(Cell::new(0i32));
        let revealed = Rc::new(Cell::new(usize::MAX));
        let geo = Rc::new(TabGeo { tabs: tabs.clone(), scroll: scroll.clone() });
        let (tb, v, hv, cl, g, rv) = (tabs.clone(), value.clone(), hover.clone(), closable.clone(), geo.clone(), revealed.clone());
        inner.draw(move |w| {
            // Keep the selected tab (and its close glyph) in view when the selection changes.
            if rv.get() != v.get() {
                rv.set(v.get());
                g.reveal(w, v.get());
            }
            g.clamp(w);
            theme::with(|t| {
                let under = under();
                let fg = t.palette.foreground;
                paint::hairline(w.x(), w.y() + w.h() - 1, w.w(), fg, under, 0.12);
                let (vx, vw) = g.view(w);
                draw::push_clip(vx, w.y(), vw, w.h());
                for (i, ((x, tw), label)) in g.tabs_at(w).into_iter().zip(tb.borrow().iter()).enumerate() {
                    if x + tw < vx || x > vx + vw { continue; }
                    let sel = v.get() == i;
                    let hot = hv.get() == i as i32;
                    let fill = if sel { Some(t.fill(&t.controls.selected, under)) } else if hot { Some(t.fill(&t.controls.hover, under)) } else { None };
                    paint::surface(x, w.y(), tw, w.h() - 1, t.radius, fill, None);
                    if sel {
                        paint::fill(x, w.y() + w.h() - 2, tw, 2, 0, t.palette.accent);
                        if crate::focus::show_ring(w) {
                            paint::stroke(x, w.y(), tw, w.h() - 1, t.radius, t.controls.focus.border_width.max(1), t.border(&t.controls.focus, under));
                        }
                    }
                    let pad = t.spacing.control_padding_x;
                    let close_w = if cl.get() { t.font.body + t.spacing.md } else { 0 };
                    paint::text_elided(label, x + pad, w.y(), tw - 2 * pad - close_w, w.h(), t.font.body, sel, if sel { fg } else { fg.darker(1.2) }, Align::Left);
                    if cl.get() && (sel || hot) {
                        paint::glyph(glyph::CROSS, x + tw - pad - t.font.body, w.y(), t.font.body, w.h(), t.font.body_small, fg.darker(1.4));
                    }
                }
                draw::pop_clip();
                if g.overflow(w) {
                    let a = g.arrow_w();
                    let (sc, max) = (g.scroll.get(), g.max_scroll(w));
                    for (i, gl) in [glyph::CHEVRON_LEFT, glyph::CHEVRON_RIGHT].iter().enumerate() {
                        let ax = if i == 0 { w.x() } else { w.x() + w.w() - a };
                        let live = if i == 0 { sc > 0 } else { sc < max };
                        let hot = hv.get() == -10 - i as i32;
                        if hot && live { paint::fill(ax, w.y(), a, w.h() - 1, t.radius, t.fill(&t.controls.hover, under)); }
                        paint::glyph(gl, ax, w.y(), a, w.h(), t.font.body, if live { fg } else { fg.darker(2.2) });
                    }
                }
            });
        });
        let (v, hv, cl, g, cc) = (value.clone(), hover.clone(), closable.clone(), geo.clone(), close.clone());
        inner.handle(move |w, ev| {
            let n = g.tabs.borrow().len();
            let pick = |i: usize| if i < n && i != v.get() { v.set(i) };
            match ev {
                Event::Enter => true,
                Event::Focus | Event::Unfocus => { w.redraw(); true }
                Event::Move => {
                    let h = match g.hit(w, app::event_x()) { Hit::Tab(i, _) => i as i32, Hit::Arrow(k) => -10 - k as i32, Hit::None => -1 };
                    if hv.get() != h { hv.set(h); w.redraw(); }
                    true
                }
                Event::Leave => { hv.set(-1); w.redraw(); true }
                Event::MouseWheel if g.overflow(w) => {
                    let d = match (app::event_dx(), app::event_dy()) {
                        (app::MouseWheel::Right, _) | (_, app::MouseWheel::Down) => 1,
                        (app::MouseWheel::Left, _) | (_, app::MouseWheel::Up) => -1,
                        _ => 0,
                    };
                    g.scroll.set(g.scroll.get() + d * 60);
                    g.clamp(w);
                    w.redraw();
                    true
                }
                Event::KeyDown if n > 0 => {
                    let cur = v.get();
                    match app::event_key() {
                        Key::Left => pick(cur.saturating_sub(1)),
                        Key::Right => pick((cur + 1).min(n - 1)),
                        Key::Home => pick(0),
                        Key::End => pick(n - 1),
                        Key::Delete if cl.get() => fire(&cc, cur),
                        _ => return false,
                    }
                    true
                }
                Event::Shortcut if n > 0 && crate::focus::visible_in_window(w) && app::is_event_ctrl() => {
                    let back = app::is_event_shift();
                    let cur = v.get();
                    match app::event_key() {
                        Key::Tab if back => pick((cur + n - 1) % n),
                        Key::Tab => pick((cur + 1) % n),
                        Key::PageUp => pick((cur + n - 1) % n),
                        Key::PageDown => pick((cur + 1) % n),
                        _ => return false,
                    }
                    true
                }
                Event::Push => {
                    let _ = w.take_focus();
                    match g.hit(w, app::event_x()) {
                        Hit::Arrow(k) => {
                            let step = (g.view(w).1 * 3 / 4).max(40);
                            g.scroll.set(g.scroll.get() + if k == 0 { -step } else { step });
                            g.clamp(w);
                            w.redraw();
                        }
                        Hit::Tab(i, on_close) => {
                            if app::event_mouse_button() == app::MouseButton::Middle || (cl.get() && on_close) { fire(&cc, i); }
                            else { v.set(i); }
                        }
                        Hit::None => {}
                    }
                    true
                }
                _ => false,
            }
        });
        inner.set_size(400, theme::with(|t| t.spacing.control_height + 2));
        let mut w2 = inner.clone();
        crate::state::bind_widget(&inner, value, false, move |_| w2.redraw());
        TabBar { inner, tabs, value: value.clone(), close, closable, geo }
    }

    /// True when the tabs are wider than the bar (arrows and scrolling on).
    pub fn overflowing(&self) -> bool {
        self.geo.overflow(&self.inner)
    }

    /// Horizontal scroll of the tab strip, in pixels.
    pub fn scroll_offset(&self) -> i32 {
        self.geo.scroll.get()
    }

    /// Screen rectangle `(x, width)` of tab `i`, clipped to the visible
    /// strip (None when it is scrolled out of view).
    pub fn tab_rect(&self, i: usize) -> Option<(i32, i32)> {
        let (vx, vw) = self.geo.view(&self.inner);
        let (x, w) = *self.geo.tabs_at(&self.inner).get(i)?;
        let (a, b) = (x.max(vx), (x + w).min(vx + vw));
        (b > a).then_some((a, b - a))
    }

    /// Shows a close glyph and calls `f(index)` (also on middle click).
    pub fn on_close(self, f: impl FnMut(usize) + 'static) -> Self {
        *self.close.borrow_mut() = Some(Box::new(f));
        self.closable.set(true);
        self
    }

    pub fn set_tabs(&mut self, tabs: &[&str]) {
        *self.tabs.borrow_mut() = tabs.iter().map(|s| s.to_string()).collect();
        if self.value.get() >= tabs.len() && !tabs.is_empty() { self.value.set(tabs.len() - 1); }
        self.inner.redraw();
    }

    pub fn tabs(&self) -> Vec<String> { self.tabs.borrow().clone() }
    pub fn selected(&self) -> usize { self.value.get() }
    /// Programmatic close request (fires on_close).
    pub fn request_close(&self, i: usize) { fire(&self.close, i); }
}
