//! In-window overlays drawn like omarchy-shell surfaces: the dropdown
//! popup, `ConfirmDialog`, notification toasts and the Omarchy `Menu`
//! (filter-as-you-type command list). Overlays are temporary widgets added
//! on top of the window and deleted when closed, so they cost nothing
//! while hidden.

use crate::paint;
use crate::state::State;
use crate::theme::{self, Rgb, Theme};
use crate::widgets::glyph;
use fltk::{app, enums::*, prelude::*, widget::Widget};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

thread_local! {
    // Deletion-tracked handles only; a layer does not own its window.
    static LAYERS: RefCell<Vec<Widget>> = const { RefCell::new(Vec::new()) };
    static REDRAW_CHECK: Cell<bool> = const { Cell::new(false) };
}

fn install_redraw_guard() {
    if !REDRAW_CHECK.with(|installed| installed.replace(true)) {
        // A PopupMenu also supports applications initialized with plain FLTK.
        // Do not depend on App::new or install a keyboard router as a side effect.
        app::add_check(|_| prepare_redraw());
    }
}

/// Called by the overlay's own shared pre-flush check. Plain FLTK windows
/// redraw damaged children independently: a canvas can therefore paint over
/// an undamaged overlay, and a translucent layer can accumulate on itself.
/// Redraw the stack together, only when an open overlay's window is dirty.
/// No timer, callback per popup, or redraw while the window is idle.
pub(crate) fn prepare_redraw() {
    LAYERS.with(|layers| {
        let mut layers = layers.borrow_mut();
        layers.retain(|l| !l.was_deleted() && l.parent().is_some());
        for layer in layers.iter().filter(|l| l.visible()) {
            if let Some(mut win) = layer.top_window() {
                if win.damage() && win.damage_type() != Damage::All {
                    win.redraw();
                }
            }
        }
    });
}

/// Keyboard policy of popups (menus, dropdowns, help): see docs/KEYBOARD.md.
/// True when Ctrl, Alt or Super is held: the key is an app shortcut, not
/// popup input (navigation, typing).
pub(crate) fn command_mods() -> bool {
    app::event_state().intersects(Shortcut::Ctrl | Shortcut::Alt | Shortcut::Meta)
}

/// `j` / `k` navigate only with no modifier at all.
pub(crate) fn plain_letter(k: Key, c: char) -> bool {
    k == Key::from_char(c) && !app::event_state().intersects(Shortcut::Ctrl | Shortcut::Alt | Shortcut::Meta | Shortcut::Shift)
}

/// Window background under overlays.
pub(crate) fn bg(t: &Theme) -> Rgb {
    t.palette.background
}

/// Adds a widget covering the anchor's window (or `rect`) on top of
/// everything; returns it. `modal` layers take keyboard focus (and give it
/// back on close).
pub(crate) fn open_layer<A: WidgetExt>(anchor: &A, rect: Option<(i32, i32, i32, i32)>, modal: bool) -> Option<Widget> {
    let win = anchor.top_window()?;
    install_redraw_guard();
    let mut group = fltk::group::Group::from_dyn_widget_ptr(win.as_widget_ptr())?;
    let prev = fltk::group::Group::try_current();
    fltk::group::Group::set_current(None::<&fltk::group::Group>);
    let mut layer = Widget::default();
    let (x, y, w, h) = rect.unwrap_or((0, 0, win.w(), win.h()));
    layer.resize(x, y, w, h);
    layer.set_frame(FrameType::NoBox);
    group.add(&layer);
    LAYERS.with(|layers| layers.borrow_mut().push(layer.clone()));
    if let Some(g) = prev {
        g.begin();
    }
    if modal {
        crate::focus::remember(&layer);
        layer.set_visible_focus();
        // Set directly: the layer's handler is installed after this returns,
        // so it could not accept FL_FOCUS yet.
        app::set_focus(&layer);
        crate::focus::sync_panels(crate::focus::PanelFocusCause::Overlay);
    }
    group.redraw();
    Some(layer)
}

/// Removes and deletes a layer; if it had focus, focus goes back to the
/// control that had it before (see [`crate::focus`]). Closing twice is a no-op.
pub(crate) fn close_layer(layer: &Widget) {
    let Some(mut parent) = layer.parent() else { return };
    // SAFETY: a Fl_Window is a Fl_Widget.
    let win = layer.top_window().map(|w| unsafe { Widget::from_widget_ptr(w.as_widget_ptr()) });
    parent.remove(layer);
    LAYERS.with(|layers| layers.borrow_mut().retain(|l| !l.was_deleted() && l.as_widget_ptr() != layer.as_widget_ptr()));
    parent.redraw();
    crate::focus::give_back(layer, win);
    // Deferred: layers close from inside their own handler, and fltk keeps
    // using the widget after it returns (a key goes on to the window).
    // fltk-rs `app::delete_widget` deletes right away.
    // SAFETY: a live widget pointer; fltk deletes it on the next loop turn.
    unsafe { Fl_delete_widget(layer.as_widget_ptr() as *mut std::ffi::c_void) }
}

extern "C" {
    // cfltk wrapper of Fl::delete_widget (linked through fltk-sys).
    fn Fl_delete_widget(w: *mut std::ffi::c_void);
}

/// Card surface with the given section style.
pub(crate) fn card(t: &Theme, x: i32, y: i32, w: i32, h: i32, surface: &theme::Surface, border_width: i32) {
    let fill = surface.fill(bg(t));
    let border = surface.border.over(fill, surface.border_alpha);
    paint::surface(x, y, w, h, t.radius, Some(fill), Some((border, border_width)));
}

// ================================================================ Dropdown

/// Omarchy `Dropdown`: optional caption label above a 28 px trigger with a
/// chevron; the popup lists options (28 px rows, hover fill) below it.
/// Keys: Enter/Space/Down open, j/k or arrows move, Enter picks, Esc closes.
#[derive(Clone)]
pub struct Dropdown {
    inner: Widget,
    options: Rc<RefCell<Vec<String>>>,
    value: State<usize>,
    label: Rc<RefCell<String>>,
}
fltk::widget_extends!(Dropdown, Widget, inner);

impl Dropdown {
    /// `Dropdown::new("Output", &["Speakers", "Headphones"], &selected_index)`.
    /// Pass an empty label to hide the caption.
    pub fn new(label: &str, options: &[&str], value: &State<usize>) -> Dropdown {
        let mut inner = Widget::default();
        inner.set_frame(FrameType::NoBox);
        inner.set_visible_focus();
        let options: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(options.iter().map(|s| s.to_string()).collect()));
        let label_rc = Rc::new(RefCell::new(label.to_string()));
        let hover = Rc::new(Cell::new(false));
        let (opts, v, lbl, hv) = (options.clone(), value.clone(), label_rc.clone(), hover.clone());
        inner.draw(move |w| {
            theme::with(|t| {
                let mut y = w.y();
                if !lbl.borrow().is_empty() {
                    let lh = t.font.caption + 4;
                    paint::text_elided(&lbl.borrow(), w.x(), y, w.w(), lh, t.font.caption, true, t.palette.foreground.darker(1.4), Align::Left);
                    y += lh + t.spacing.label_gap;
                }
                let h = t.spacing.control_height;
                let focused = crate::focus::show_ring(w);
                let st = if focused { &t.controls.focus } else if hv.get() { &t.controls.hover } else { &t.controls.normal };
                let under = bg(t);
                paint::surface(w.x(), y, w.w(), h, t.radius, Some(t.fill(st, under)), Some((t.border(st, under), st.border_width)));
                let fg = t.palette.foreground;
                let pad = t.spacing.control_padding_x + 1;
                let chev = paint::measure(glyph::CHEVRON_DOWN, t.font.body, false);
                let text = opts.borrow().get(v.get()).cloned().unwrap_or_default();
                paint::text_elided(&text, w.x() + pad, y, w.w() - 2 * pad - chev - t.spacing.md, h, t.font.body, false, fg, Align::Left);
                paint::glyph(glyph::CHEVRON_DOWN, w.x() + w.w() - 9 - chev, y, chev, h, t.font.body, fg.darker(1.2));
            });
        });
        let (opts, v, hv) = (options.clone(), value.clone(), hover.clone());
        inner.handle(move |w, ev| match ev {
            Event::Enter => { hv.set(true); w.redraw(); true }
            Event::Leave => { hv.set(false); w.redraw(); true }
            Event::Focus | Event::Unfocus => { w.redraw(); true }
            Event::Push => { let _ = w.take_focus(); open_popup(w, &opts, &v); true }
            Event::KeyDown if matches!(app::event_key(), Key::Enter | Key::KPEnter | Key::Down) || app::event_key() == Key::from_char(' ') => {
                open_popup(w, &opts, &v);
                true
            }
            _ => false,
        });
        let (w, h) = theme::with(|t| {
            let lh = if label.is_empty() { 0 } else { t.font.caption + 4 + t.spacing.label_gap };
            (t.spacing.dropdown_width, t.spacing.control_height + lh)
        });
        inner.set_size(w, h);
        let mut w2 = inner.clone();
        crate::state::bind_widget(&inner, value, false, move |_| w2.redraw());
        Dropdown { inner, options, value: value.clone(), label: label_rc }
    }

    pub fn selected(&self) -> usize {
        self.value.get()
    }

    pub fn selected_text(&self) -> Option<String> {
        self.options.borrow().get(self.value.get()).cloned()
    }

    pub fn set_options(&mut self, options: &[&str]) {
        *self.options.borrow_mut() = options.iter().map(|s| s.to_string()).collect();
        self.inner.redraw();
    }

    pub fn label(&self) -> String {
        self.label.borrow().clone()
    }

    /// Opens the popup (programmatic; same as clicking the trigger).
    pub fn open(&mut self) {
        let (o, v) = (self.options.clone(), self.value.clone());
        open_popup(&mut self.inner, &o, &v);
    }
}

fn open_popup(trigger: &mut Widget, options: &Rc<RefCell<Vec<String>>>, value: &State<usize>) {
    let n = options.borrow().len() as i32;
    if n == 0 {
        return;
    }
    let (row_h, gap, has_label) = theme::with(|t| (t.spacing.popup_row_height, t.spacing.sm, t.font.caption));
    let label_h = trigger.h() - theme::with(|t| t.spacing.control_height);
    let _ = has_label;
    let shown = n.min(8);
    let h = shown * row_h + (shown - 1) * gap + 2 + 2 * 2;
    let y = trigger.y() + trigger.h() + 2;
    let win_h = trigger.top_window().map(|w| w.h()).unwrap_or(y + h);
    let y = if y + h > win_h { (trigger.y() + label_h - 2 - h).max(0) } else { y };
    let rect = (trigger.x(), y, trigger.w(), h);
    let Some(mut layer) = open_layer(trigger, None, true) else { return };
    let current = Rc::new(Cell::new(value.get() as i32));
    let top = Rc::new(Cell::new(0i32));
    let (opts, cur, tp) = (options.clone(), current.clone(), top.clone());
    layer.draw(move |_| {
        theme::with(|t| {
            let (x, y, w, h) = rect;
            card(t, x, y, w, h, &t.popups, t.controls.normal.border_width.max(1));
            let fill = t.popups.fill(bg(t));
            let inset = 2;
            for i in 0..shown {
                let idx = tp.get() + i;
                let Some(text) = opts.borrow().get(idx as usize).cloned() else { break };
                let ry = y + inset + i * (row_h + gap);
                if idx == cur.get() {
                    paint::fill(x + inset, ry, w - 2 * inset, row_h, t.radius, t.fill(&t.controls.hover, fill));
                }
                paint::text_elided(&text, x + inset + t.spacing.control_padding_x, ry, w - 2 * (inset + t.spacing.control_padding_x), row_h, t.font.body, false, t.popups.text, Align::Left);
            }
        });
    });
    let (v, cur, tp) = (value.clone(), current.clone(), top.clone());
    let mut trig = trigger.clone();
    layer.handle(move |l, ev| {
        let (x, y, w, h) = rect;
        let row_at = |ey: i32| -> Option<i32> {
            if app::event_x() < x || app::event_x() >= x + w || ey < y || ey >= y + h {
                return None;
            }
            let i = (ey - y - 2) / (row_h + gap);
            (i >= 0 && i < shown && tp.get() + i < n).then_some(tp.get() + i)
        };
        let ensure = |c: i32| {
            if c < tp.get() { tp.set(c) } else if c >= tp.get() + shown { tp.set(c - shown + 1) }
        };
        let finish = |l: &Widget, pick: Option<i32>, trig: &mut Widget| {
            if let Some(i) = pick {
                v.set(i as usize);
            }
            close_layer(l);
            let _ = trig.take_focus();
            trig.redraw();
        };
        match ev {
            Event::Move | Event::Drag => {
                if let Some(i) = row_at(app::event_y()) { cur.set(i); l.redraw(); }
                true
            }
            Event::Push => {
                let pick = row_at(app::event_y());
                finish(l, pick, &mut trig);
                true
            }
            Event::MouseWheel => {
                let d = app::event_dy_value().saturating_mul(1);
                tp.set((tp.get() + d).clamp(0, (n - shown).max(0)));
                l.redraw();
                true
            }
            Event::KeyDown => {
                let k = app::event_key();
                let cmd = command_mods();
                if k == Key::Escape && !cmd {
                    finish(l, None, &mut trig);
                } else if k == Key::Tab {
                    // stays in the popup
                } else if (k == Key::Down && !cmd) || plain_letter(k, 'j') {
                    cur.set((cur.get() + 1).min(n - 1)); ensure(cur.get()); l.redraw();
                } else if (k == Key::Up && !cmd) || plain_letter(k, 'k') {
                    cur.set((cur.get() - 1).max(0)); ensure(cur.get()); l.redraw();
                } else if matches!(k, Key::Enter | Key::KPEnter) && !cmd {
                    let c = cur.get();
                    finish(l, Some(c), &mut trig);
                } else {
                    // Not a dropdown key: close, then the key reaches the app.
                    finish(l, None, &mut trig);
                    return false;
                }
                true
            }
            Event::Focus | Event::Enter => true,
            Event::Unfocus => {
                // Focus went elsewhere (another layer, a click): close on the
                // next loop turn, never inside fltk's focus change, and leave
                // focus where it went.
                let l2 = l.clone();
                app::add_timeout3(0.0, move |_| if !l2.was_deleted() { close_layer(&l2) });
                true
            }
            _ => false,
        }
    });
    let c = current.get();
    if c >= shown {
        top.set(c - shown + 1);
    }
}

// =========================================================== ConfirmDialog

/// Omarchy `ConfirmDialog`: scrim `rgba(bg, .7)`, centered 370 px card with
/// an accent border, message and Cancel / Confirm buttons (confirm is
/// selected by default; destructive styles it with the urgent color).
/// Keys: Left/Right/Tab switch, Enter activates, Esc cancels.
pub struct ConfirmDialog;

impl ConfirmDialog {
    pub fn open<A: WidgetExt>(anchor: &A, message: &str, confirm: &str, destructive: bool, on_close: impl FnOnce(bool) + 'static) {
        let Some(mut layer) = open_layer(anchor, None, true) else { return };
        let selected = Rc::new(Cell::new(1i32));
        let msg = message.to_string();
        let labels = ["Cancel".to_string(), confirm.to_string()];
        let geom = Rc::new(move |l: &Widget| -> ((i32, i32, i32, i32), [(i32, i32, i32, i32); 2]) {
            theme::with(|t| {
                let s = |px: f64| (px * t.font.base as f64 / 12.0).round() as i32;
                let cw = (l.w() - s(32.0)).min(s(370.0));
                let pad = s(18.0);
                fltk::draw::set_font(Font::Helvetica, t.font.title);
                let (_, mh) = fltk::draw::measure(&msg, true);
                let mh = mh.max(t.font.title + 4);
                let ch = 2 * pad + mh + s(20.0) + s(34.0);
                let cx = (l.w() - cw) / 2;
                let cy = (l.h() - ch) / 2;
                let (bw, bh) = (s(88.0), s(34.0));
                let by = cy + ch - pad - bh;
                let b1 = (cx + cw - pad - bw, by, bw, bh);
                let b0 = (b1.0 - s(10.0) - bw, by, bw, bh);
                ((cx, cy, cw, ch), [b0, b1])
            })
        });
        let (sel, msg2, labels2, geom2) = (selected.clone(), message.to_string(), labels.clone(), geom.clone());
        layer.draw(move |l| {
            if let Some(win) = l.top_window() {
                if (l.w(), l.h()) != (win.w(), win.h()) { l.resize(0, 0, win.w(), win.h()); }
            }
            let ((cx, cy, cw, ch), buttons) = geom2(l);
            theme::with(|t| {
                let b = bg(t);
                // Scrim: background at 0.7 over the (unknown) content: approximate
                // by darkening the window background area.
                fltk::draw::set_draw_color(b.fl());
                draw_scrim(l, b, 0.7);
                let border = t.menu.selected_text;
                paint::surface(cx, cy, cw, ch, t.radius, Some(b), Some((border, t.controls.normal.border_width.max(1))));
                let pad = (18.0 * t.font.base as f64 / 12.0).round() as i32;
                paint::text(&msg2, cx + pad, cy + pad, cw - 2 * pad, ch - 2 * pad, t.font.title, false, t.palette.foreground, Align::TopLeft | Align::Wrap);
                for (i, (bx, by, bw, bh)) in buttons.iter().copied().enumerate() {
                    let on = sel.get() == i as i32;
                    let fg = t.palette.foreground;
                    let (fill, border, text) = if i == 0 {
                        if on { (Some(fg.over(b, 0.08)), t.menu.selected_text, t.menu.selected_text) } else { (None, fg.over(b, 0.38), fg) }
                    } else if destructive {
                        let u = t.palette.urgent;
                        if on { (Some(u.over(b, 0.22)), u, u) } else { (None, u.over(b, 0.56), fg) }
                    } else if on {
                        (Some(fg.over(b, 0.08)), t.menu.selected_text, t.menu.selected_text)
                    } else {
                        (None, fg.over(b, 0.38), fg)
                    };
                    paint::surface(bx, by, bw, bh, 0, fill, Some((border, 1)));
                    paint::text(&labels2[i], bx, by, bw, bh, t.font.caption, false, text, Align::Center);
                }
            });
        });
        let on_close = Rc::new(RefCell::new(Some(Box::new(on_close) as Box<dyn FnOnce(bool)>)));
        let sel = selected.clone();
        layer.handle(move |l, ev| {
            let ((cx, cy, cw, ch), buttons) = geom(l);
            let hit = |b: (i32, i32, i32, i32)| {
                let (x, y) = app::event_coords();
                x >= b.0 && x < b.0 + b.2 && y >= b.1 && y < b.1 + b.3
            };
            let done = |l: &Widget, ok: bool| {
                close_layer(l);
                if let Some(f) = on_close.borrow_mut().take() {
                    f(ok);
                }
            };
            match ev {
                Event::Move => {
                    for (i, b) in buttons.iter().enumerate() {
                        if hit(*b) && sel.get() != i as i32 { sel.set(i as i32); l.redraw(); }
                    }
                    true
                }
                Event::Push => {
                    let (x, y) = app::event_coords();
                    if hit(buttons[1]) { done(l, true) }
                    else if hit(buttons[0]) || !(x >= cx && x < cx + cw && y >= cy && y < cy + ch) { done(l, false) }
                    true
                }
                Event::KeyDown => {
                    match app::event_key() {
                        Key::Escape => done(l, false),
                        Key::Left | Key::Right | Key::Tab => { sel.set(1 - sel.get()); l.redraw(); }
                        Key::Enter | Key::KPEnter => { let ok = sel.get() == 1; done(l, ok) }
                        _ => {}
                    }
                    true
                }
                Event::Focus | Event::Enter | Event::Shortcut => true,
                _ => false,
            }
        });
    }
}

/// Real alpha fill over whatever is already drawn: FLTK has no alpha
/// rectangles, but it alpha-blends RGBA images, so a 1×1 RGBA image is
/// scaled over the area.
pub fn draw_scrim(l: &Widget, color: Rgb, alpha: f64) {
    paint::alpha_fill(l.x(), l.y(), l.w(), l.h(), color, alpha);
}

// ============================================================ Notification

/// Omarchy notification card (toast) at the window's top-right: icon slot,
/// bold summary, body in `darker(text, 1.15)`, accent border, auto-dismiss
/// (8 s normal, never for critical). A click dismisses it; from the
/// keyboard, Tab reaches it and Enter, Space, Delete or Esc dismiss it.
pub struct Notification;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Low,
    Normal,
    Critical,
}

thread_local! {
    static TOASTS: RefCell<Vec<Widget>> = const { RefCell::new(Vec::new()) };
}

impl Notification {
    pub fn show<A: WidgetExt>(anchor: &A, icon: &str, summary: &str, body: &str, urgency: Urgency) {
        let Some(win) = anchor.top_window() else { return };
        let (cw, ch) = theme::with(|t| {
            let s = |px: f64| (px * t.font.base as f64 / 12.0).round() as i32;
            let cw = s(380.0).min(win.w() - 2 * t.gaps_out.max(8));
            let lines = if body.is_empty() { 1 } else { 2 };
            let pad = if body.is_empty() { s(7.0) } else { s(10.0) };
            (cw, (2 * pad + lines * (t.font.title + 6)).max(s(40.0) + 2 * pad) + 4)
        });
        let margin = theme::with(|t| t.gaps_out.max(8));
        let stack_y = TOASTS.with(|v| v.borrow().iter().map(|w| w.h() + margin).sum::<i32>());
        let rect = (win.w() - cw - margin, margin + stack_y, cw, ch);
        let Some(mut layer) = open_layer(anchor, Some(rect), false) else { return };
        TOASTS.with(|v| v.borrow_mut().push(layer.clone()));
        let (icon, summary, body) = (icon.to_string(), summary.to_string(), body.to_string());
        layer.draw(move |l| {
            theme::with(|t| {
                let n = &t.notifications;
                card(t, l.x(), l.y(), l.w(), l.h(), n, 2);
                let s = |px: f64| (px * t.font.base as f64 / 12.0).round() as i32;
                let mut x = l.x() + 12;
                if !icon.is_empty() {
                    let slot = s(40.0);
                    paint::glyph(&icon, x, l.y(), slot, l.h(), t.font.display_large, n.text);
                    x += slot + 12;
                }
                let w = l.x() + l.w() - s(10.0) - x;
                let line = t.font.title + 6;
                let top = l.y() + (l.h() - if body.is_empty() { line } else { 2 * line }) / 2;
                paint::text_elided(&summary, x, top, w, line, t.font.title, true, n.text, Align::Left);
                if !body.is_empty() {
                    paint::text_elided(&body, x, top + line, w, line, t.font.title, false, n.text.darker(1.15), Align::Left);
                }
                if crate::focus::show_ring(l) {
                    let fill = n.fill(bg(t));
                    paint::stroke(l.x() + 3, l.y() + 3, l.w() - 6, l.h() - 6, (t.radius - 3).max(0), 1, t.border(&t.controls.focus, fill));
                }
            });
        });
        // Keyboard: Tab reaches the toast; Enter, Space, Delete or Esc dismiss it.
        layer.set_visible_focus();
        layer.handle(|l, ev| match ev {
            Event::Push => { dismiss(l); true }
            Event::Enter => true,
            Event::Focus => { crate::focus::remember(l); l.redraw(); true }
            Event::Unfocus => { l.redraw(); true }
            Event::KeyDown => {
                let k = app::event_key();
                if matches!(k, Key::Enter | Key::KPEnter | Key::Escape | Key::Delete | Key::BackSpace) || k == Key::from_char(' ') {
                    dismiss(l);
                    return true;
                }
                false
            }
            _ => false,
        });
        let timeout = match urgency { Urgency::Low => Some(5.0), Urgency::Normal => Some(8.0), Urgency::Critical => None };
        if let Some(secs) = timeout {
            let l2 = layer.clone();
            app::add_timeout3(secs, move |_| dismiss(&l2));
        }
    }

    /// Number of toasts currently shown (all windows).
    pub fn count() -> usize {
        TOASTS.with(|v| v.borrow().len())
    }
}

fn dismiss(l: &Widget) {
    let present = TOASTS.with(|v| {
        let mut v = v.borrow_mut();
        let before = v.len();
        v.retain(|w| w.as_widget_ptr() != l.as_widget_ptr());
        before != v.len()
    });
    if present {
        close_layer(l);
    }
}

// ==================================================================== Menu

/// One entry of an Omarchy [`Menu`].
#[derive(Debug, Clone, PartialEq)]
pub struct MenuItem {
    pub icon: String,
    pub label: String,
    pub detail: String,
    pub submenu: bool,
    pub enabled: bool,
}

impl MenuItem {
    pub fn new(icon: &str, label: &str) -> MenuItem {
        MenuItem { icon: icon.into(), label: label.into(), detail: String::new(), submenu: false, enabled: true }
    }
    pub fn detail(mut self, d: &str) -> Self { self.detail = d.into(); self }
    /// Shows the "›" trail.
    pub fn submenu(mut self) -> Self { self.submenu = true; self }
    pub fn disabled(mut self) -> Self { self.enabled = false; self }
}

/// The Omarchy menu card: centered over a scrim, the header shows the
/// typed filter (or "Title…" dimmed), 50 px rows with an 18 px glyph,
/// accent text on the cursor row. Type to filter, Up/Down/PageUp/PageDown
/// move (wrapping, skipping disabled), Enter picks, Esc clears/closes.
pub struct Menu;

pub(crate) fn menu_filter(items: &[MenuItem], q: &str) -> Vec<usize> {
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return (0..items.len()).collect();
    }
    let mut scored: Vec<(u32, usize)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, it)| {
            let label = it.label.to_lowercase();
            let detail = it.detail.to_lowercase();
            let terms_ok = q.split_whitespace().all(|term| label.contains(term) || detail.split_whitespace().any(|w| w == term));
            if !terms_ok {
                return None;
            }
            let tier = if label == q { 0 } else if label.starts_with(&q) { 10 } else if label.contains(&q) { 30 } else { 60 };
            Some((tier * 1000 + i as u32, i))
        })
        .collect();
    scored.sort();
    scored.into_iter().map(|(_, i)| i).collect()
}

impl Menu {
    /// Opens the menu; `on_pick(index)` receives the index into `items`.
    pub fn open<A: WidgetExt>(anchor: &A, title: &str, items: Vec<MenuItem>, on_pick: impl FnOnce(usize) + 'static) {
        let Some(mut layer) = open_layer(anchor, None, true) else { return };
        let items = Rc::new(items);
        let filter = Rc::new(RefCell::new(String::new()));
        let cursor = Rc::new(Cell::new(0usize)); // index into visible list
        let top = Rc::new(Cell::new(0usize));
        let visible = Rc::new(RefCell::new(menu_filter(&items, "")));
        let title = title.to_string();

        struct Geo { cx: i32, cy: i32, cw: i32, ch: i32, pad: i32, header: i32, gap: i32, row: i32, spacing: i32, rows: usize }
        let geo = {
            let visible = visible.clone();
            move |l: &Widget| -> Geo {
                theme::with(|t| {
                    let s = |px: f64| (px * t.font.base as f64 / 12.0).round() as i32;
                    let cw = s(300.0).min(l.w() - 2 * t.gaps_out);
                    let pad = t.spacing.panel_padding;
                    let header = s(34.0).max(t.font.heading + 12);
                    let gap = t.spacing.md;
                    let row = s(50.0).max(t.font.heading + 24);
                    let spacing = t.spacing.xs;
                    let avail = ((l.h() as f64 * 0.7) as i32).min(l.h() - 2 * t.gaps_out - 2 * pad - header - gap);
                    let n = visible.borrow().len().max(1) as i32;
                    let full = n * row + (n - 1) * spacing;
                    let (list_h, rows) = if full <= avail {
                        (full, n as usize)
                    } else {
                        let k = ((avail - 28 - spacing) / (row + spacing)).max(1);
                        (k * (row + spacing) + (row as f64 * 0.55).round() as i32, k as usize + 1)
                    };
                    let ch = 2 * pad + header + gap + list_h;
                    Geo { cx: (l.w() - cw) / 2, cy: ((l.h() - ch) / 2).max(t.gaps_out), cw, ch, pad, header, gap, row, spacing, rows }
                })
            }
        };

        let (it, fl, cu, tp, vis, g2) = (items.clone(), filter.clone(), cursor.clone(), top.clone(), visible.clone(), geo.clone());
        layer.draw(move |l| {
            if let Some(win) = l.top_window() {
                if (l.w(), l.h()) != (win.w(), win.h()) { l.resize(0, 0, win.w(), win.h()); }
            }
            let g = g2(l);
            theme::with(|t| {
                let m = &t.menu;
                draw_scrim(l, m.background, 0.5);
                let fill = m.fill(bg(t));
                paint::surface(g.cx, g.cy, g.cw, g.ch, t.radius, Some(fill), Some((m.border.over(fill, m.border_alpha), 2)));
                let x = g.cx + g.pad;
                let w = g.cw - 2 * g.pad;
                let q = fl.borrow();
                let (htext, alpha) = if q.is_empty() { (format!("{title}…"), 0.58) } else { (q.clone(), 1.0) };
                paint::text_elided(&htext, x, g.cy + g.pad, w, g.header, t.font.heading, false, m.text.over(fill, alpha), Align::Left);
                let ly = g.cy + g.pad + g.header + g.gap;
                let vis = vis.borrow();
                if vis.is_empty() {
                    let msg = if q.is_empty() { "Nothing here yet".to_string() } else { format!("No matches for “{q}”") };
                    paint::glyph(glyph::EMPTY, x, ly, w, g.row, t.font.display_large, m.selected_text.over(fill, 0.8));
                    paint::text(&msg, x, ly + g.row - 6, w, t.font.title + 6, t.font.title, false, m.text.over(fill, 0.7), Align::Center);
                    return;
                }
                fltk::draw::push_clip(x, ly, w, g.ch - (ly - g.cy) - g.pad);
                for slot in 0..g.rows {
                    let vi = tp.get() + slot;
                    let Some(&idx) = vis.get(vi) else { break };
                    let item = &it[idx];
                    let ry = ly + slot as i32 * (g.row + g.spacing);
                    let on = vi == cu.get();
                    if on {
                        let sel = m.selected_background.over(fill, m.selected_background_alpha);
                        let border = (m.selected_border_alpha > 0.0).then(|| (m.selected_border.over(sel, m.selected_border_alpha), 1));
                        paint::surface(x, ry, w, g.row, t.radius, Some(sel), border);
                    }
                    let mut color = if on { m.selected_text } else { m.text };
                    if !item.enabled { color = color.over(fill, 0.4); }
                    let s = |px: f64| (px * t.font.base as f64 / 12.0).round() as i32;
                    let mut tx = x + s(18.0);
                    if !item.icon.is_empty() {
                        paint::glyph(&item.icon, x + s(8.0), ry, s(36.0), g.row, t.font.icon_large, color);
                        tx = x + s(8.0) + s(36.0) + s(6.0);
                    }
                    let trail = if item.submenu { s(14.0) + s(8.0) } else { 0 };
                    let tw = x + w - trail - s(6.0) - tx;
                    let show_detail = !item.detail.is_empty() && !q.is_empty();
                    let lh = t.font.heading + 6;
                    if show_detail {
                        let dh = t.font.body_small + 4;
                        let top_y = ry + (g.row - lh - dh - s(3.0)) / 2;
                        paint::text_elided(&item.label, tx, top_y, tw, lh, t.font.heading, false, color, Align::Left);
                        paint::text_elided(&item.detail, tx, top_y + lh + s(3.0), tw, dh, t.font.body_small, false, m.text.over(fill, 0.52), Align::Left);
                    } else {
                        paint::text_elided(&item.label, tx, ry, tw, g.row, t.font.heading, false, color, Align::Left);
                    }
                    if item.submenu {
                        paint::glyph(glyph::SUBMENU, x + w - s(8.0) - s(14.0), ry, s(14.0), g.row, t.font.heading, m.text.over(fill, 0.36));
                    }
                }
                fltk::draw::pop_clip();
            });
        });

        let on_pick = Rc::new(RefCell::new(Some(Box::new(on_pick) as Box<dyn FnOnce(usize)>)));
        let (it, fl, cu, tp, vis) = (items.clone(), filter.clone(), cursor.clone(), top.clone(), visible.clone());
        layer.handle(move |l, ev| {
            let g = geo(l);
            let refilter = |l: &mut Widget| {
                *vis.borrow_mut() = menu_filter(&it, &fl.borrow());
                cu.set(0);
                tp.set(0);
                l.redraw();
            };
            let step = |d: i32| {
                let n = vis.borrow().len() as i32;
                if n == 0 { return; }
                let mut c = cu.get() as i32;
                for _ in 0..n {
                    c = (c + d).rem_euclid(n);
                    if it[vis.borrow()[c as usize]].enabled { break; }
                }
                cu.set(c as usize);
                let full = g.rows.saturating_sub(1).max(1);
                if (c as usize) < tp.get() { tp.set(c as usize) } else if c as usize >= tp.get() + full { tp.set(c as usize + 1 - full) }
            };
            let pick = |l: &Widget| {
                let Some(&idx) = vis.borrow().get(cu.get()) else { return };
                if !it[idx].enabled { return; }
                close_layer(l);
                if let Some(f) = on_pick.borrow_mut().take() { f(idx); }
            };
            let row_at = |ey: i32| -> Option<usize> {
                let ly = g.cy + g.pad + g.header + g.gap;
                let (ex, _) = app::event_coords();
                if ex < g.cx + g.pad || ex >= g.cx + g.cw - g.pad || ey < ly { return None; }
                let slot = ((ey - ly) / (g.row + g.spacing)) as usize;
                let vi = tp.get() + slot;
                (slot < g.rows && vi < vis.borrow().len()).then_some(vi)
            };
            match ev {
                Event::KeyDown => {
                    let k = app::event_key();
                    let ctrl = app::is_event_ctrl();
                    match k {
                        Key::Escape => {
                            if fl.borrow().is_empty() { close_layer(l); } else { fl.borrow_mut().clear(); refilter(l); }
                        }
                        Key::Down => { step(1); l.redraw(); }
                        Key::Up => { step(-1); l.redraw(); }
                        Key::PageDown => { step(6); l.redraw(); }
                        Key::PageUp => { step(-6); l.redraw(); }
                        Key::Enter | Key::KPEnter | Key::Right => pick(l),
                        Key::BackSpace => {
                            if ctrl {
                                let mut f = fl.borrow_mut();
                                let keep = f.trim_end().rfind(' ').map(|i| i + 1).unwrap_or(0);
                                f.truncate(keep);
                            } else {
                                fl.borrow_mut().pop();
                            }
                            refilter(l);
                        }
                        _ if ctrl && k == Key::from_char('u') => { fl.borrow_mut().clear(); refilter(l); }
                        Key::Tab => {}
                        _ => {
                            let text = app::event_text();
                            if !command_mods() && !text.is_empty() && text.chars().all(|c| !c.is_control()) {
                                fl.borrow_mut().push_str(&text);
                                refilter(l);
                            } else {
                                // Not menu input (F1, F10, Ctrl+…): close and let the app have it.
                                close_layer(l);
                                return false;
                            }
                        }
                    }
                    true
                }
                Event::Move => {
                    if let Some(vi) = row_at(app::event_y()) { if vi != cu.get() { cu.set(vi); l.redraw(); } }
                    true
                }
                Event::Push => {
                    let (x, y) = app::event_coords();
                    if x < g.cx || x >= g.cx + g.cw || y < g.cy || y >= g.cy + g.ch {
                        close_layer(l);
                    } else if let Some(vi) = row_at(y) {
                        cu.set(vi);
                        pick(l);
                    }
                    true
                }
                Event::MouseWheel => {
                    let d = app::event_dy_value().saturating_mul(1);
                    let n = vis.borrow().len();
                    let max_top = n.saturating_sub(g.rows.saturating_sub(1));
                    tp.set((tp.get() as i32 + d).clamp(0, max_top as i32) as usize);
                    l.redraw();
                    true
                }
                Event::Focus | Event::Enter => true,
                _ => false,
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_filter_ranks_exact_prefix_contains() {
        let items = vec![
            MenuItem::new("", "Screenshot"),
            MenuItem::new("", "Screen record"),
            MenuItem::new("", "Theme").detail("change colors"),
            MenuItem::new("", "Record"),
        ];
        assert_eq!(menu_filter(&items, ""), vec![0, 1, 2, 3]);
        assert_eq!(menu_filter(&items, "record"), vec![3, 1]);
        assert_eq!(menu_filter(&items, "colors"), vec![2]);
        assert!(menu_filter(&items, "zzz").is_empty());
    }
}
