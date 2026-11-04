//! `AppWindow`: an Omarchy application window = header (title + actions)
//! over a padded body column. Normal (tiled) by default; `Mode::Floating`
//! floats it on Hyprland at the requested size, the header drags it and
//! the bottom corners resize it — all through Hyprland IPC on this
//! window only.

use crate::paint;
use crate::theme;
use crate::widgets::{measure_offscreen, ActionButton};
use crate::{hyprland, png};
use fltk::{app, enums::*, group::Flex, prelude::*, widget::Widget, window::DoubleWindow};
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The compositor decides (tiled on Omarchy).
    Normal,
    /// Floating at the requested size; header drag + corner resize.
    Floating,
    /// Native fullscreen (FLTK); leave it through the app's own controls.
    Fullscreen,
}

impl Mode {
    /// Parses the config value used by Files' `window_mode` and similar.
    pub fn parse(s: &str) -> Option<Mode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "normal" => Some(Mode::Normal),
            "floating" | "float" => Some(Mode::Floating),
            "fullscreen" => Some(Mode::Fullscreen),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Normal => "normal",
            Mode::Floating => "floating",
            Mode::Fullscreen => "fullscreen",
        }
    }
}

const GRIP: i32 = 14;

#[derive(Clone)]
pub struct AppWindow {
    win: DoubleWindow,
    header: Header,
    body: Flex,
    mode: Rc<Cell<Mode>>,
    size: (i32, i32),
    min: Rc<Cell<(i32, i32)>>,
    shortcuts: Rc<RefCell<Vec<(crate::keys::Keys, Box<dyn FnMut()>)>>>,
}

impl std::ops::Deref for AppWindow {
    type Target = DoubleWindow;
    fn deref(&self) -> &DoubleWindow {
        &self.win
    }
}
impl std::ops::DerefMut for AppWindow {
    fn deref_mut(&mut self) -> &mut DoubleWindow {
        &mut self.win
    }
}

struct Drag {
    address: String,
    origin: (i32, i32),
    size: (i32, i32),
    cursor: (i32, i32),
    corner: u8, // 0 move, 1 bottom-left, 2 bottom-right
    last: std::time::Instant,
}

thread_local! {
    static DRAG: RefCell<Option<Drag>> = const { RefCell::new(None) };
}

fn drag_begin(title: &str, corner: u8) {
    let Some(c) = hyprland::own_clients(Some(title)).into_iter().next() else { return };
    let Some(cursor) = hyprland::cursor() else { return };
    DRAG.with(|d| *d.borrow_mut() = Some(Drag { address: c.address, origin: c.at, size: c.size, cursor, corner, last: std::time::Instant::now() }));
}

fn drag_update() {
    DRAG.with(|d| {
        let mut d = d.borrow_mut();
        let Some(st) = d.as_mut() else { return };
        if st.last.elapsed().as_millis() < 16 {
            return;
        }
        st.last = std::time::Instant::now();
        let Some((cx, cy)) = hyprland::cursor() else { return };
        let (dx, dy) = (cx - st.cursor.0, cy - st.cursor.1);
        match st.corner {
            0 => { hyprland::move_to(&st.address, st.origin.0 + dx, st.origin.1 + dy); }
            1 => {
                hyprland::move_to(&st.address, st.origin.0 + dx, st.origin.1);
                hyprland::resize_to(&st.address, st.size.0 - dx, st.size.1 + dy);
            }
            _ => { hyprland::resize_to(&st.address, st.size.0 + dx, st.size.1 + dy); }
        }
    });
}

fn drag_end() {
    DRAG.with(|d| *d.borrow_mut() = None);
}

impl AppWindow {
    /// Creates the window and leaves its **body** open, so widgets created
    /// next are added to the body column. Call [`AppWindow::end`] when done.
    pub fn new(title: &str, width: i32, height: i32) -> AppWindow {
        let t = theme::current();
        let mut win = DoubleWindow::default().with_size(width, height).with_label(title);
        win.set_color(t.palette.background.fl());
        win.size_range((width / 2).max(200), (height / 2).max(150), 0, 0);
        let mut root = Flex::default_fill().column();
        root.set_spacing(0);
        let header = Header::new(title);
        root.fixed(&*header, header.h());
        let mut body = Flex::default().column();
        body.set_margin(t.spacing.panel_padding);
        body.set_spacing(t.spacing.panel_gap);
        root.end();
        win.resizable(&root);
        win.end();
        body.begin();

        let mode = Rc::new(Cell::new(Mode::Normal));
        let shortcuts: Rc<RefCell<Vec<(crate::keys::Keys, Box<dyn FnMut()>)>>> = Rc::default();
        let (m2, sc, ttl) = (mode.clone(), shortcuts.clone(), title.to_string());
        win.handle(move |w, ev| {
            let floating = m2.get() == Mode::Floating;
            match ev {
                Event::Push if floating && app::event_y() >= w.h() - GRIP => {
                    let corner = if app::event_x() < GRIP { 1 } else if app::event_x() >= w.w() - GRIP { 2 } else { 0 };
                    if corner > 0 {
                        // Native compositor resize first (any Wayland compositor), Hyprland IPC otherwise.
                        let edge = if corner == 1 { crate::native::Edge::BottomLeft } else { crate::native::Edge::BottomRight };
                        if !crate::native::begin_resize(w, edge) {
                            drag_begin(&ttl, corner);
                        }
                        return true;
                    }
                    false
                }
                Event::Drag if DRAG.with(|d| d.borrow().as_ref().is_some_and(|d| d.corner > 0)) => { drag_update(); true }
                Event::Released if DRAG.with(|d| d.borrow().is_some()) => { drag_end(); true }
                Event::Shortcut | Event::KeyDown => {
                    let mut list = sc.borrow_mut();
                    for (k, f) in list.iter_mut() {
                        if k.matches_event() {
                            f();
                            return true;
                        }
                    }
                    false
                }
                _ => false,
            }
        });
        let m3 = mode.clone();
        win.draw(move |w| {
            theme::with(|t| paint::fill(0, 0, w.w(), w.h(), 0, t.palette.background));
            draw_stack(w);
            if m3.get() == Mode::Floating {
                theme::with(|t| {
                    let c = t.palette.foreground.darker(1.4).fl();
                    let (b, s) = (w.h() - 3, 9);
                    fltk::draw::set_draw_color(c);
                    fltk::draw::draw_polygon(w.w() - 3, b - s, w.w() - 3, b, w.w() - 3 - s, b);
                    fltk::draw::draw_polygon(3, b - s, 3, b, 3 + s, b);
                });
            }
        });
        // cfltk runs FLTK's own draw before this closure by default: it would
        // draw damaged children first and this background would then cover them.
        win.super_draw(false);
        // Escape does not close Omarchy app windows by default.
        win.set_callback(|w| {
            if app::event() == Event::Close {
                w.hide();
            }
        });
        crate::theme::on_change_while(&win, {
            let mut w = win.clone();
            move |t| { w.set_color(t.palette.background.fl()); w.redraw(); }
        });
        let min = Rc::new(Cell::new(((width / 2).max(200), (height / 2).max(150))));
        AppWindow { win, header, body, mode, size: (width, height), min, shortcuts }
    }

    /// Closes the body column (and the window group).
    pub fn end(&mut self) {
        self.body.end();
        self.win.end();
    }

    pub fn header(&self) -> Header {
        self.header.clone()
    }

    /// The padded column under the header (panel padding 18, gap 14).
    pub fn body(&self) -> Flex {
        self.body.clone()
    }

    pub fn set_min_size(&mut self, w: i32, h: i32) {
        self.min.set((w, h));
        self.win.size_range(w, h, 0, 0);
    }

    pub fn mode(&self) -> Mode {
        self.mode.get()
    }

    /// Switches mode; when shown on Hyprland, floats/tiles this window now.
    pub fn set_mode(&mut self, mode: Mode) {
        let previous = self.mode.replace(mode);
        self.header.set_draggable(mode == Mode::Floating);
        if self.win.shown() {
            if mode == Mode::Fullscreen {
                self.win.fullscreen(true);
                self.win.redraw();
                return;
            }
            if previous == Mode::Fullscreen || self.win.fullscreen_active() {
                self.win.fullscreen(false);
            }
        }
        if self.win.shown() && hyprland::available() {
            let title = self.win.label();
            let size = self.size;
            if let Some(c) = hyprland::own_clients(Some(&title)).into_iter().next() {
                hyprland::set_floating(&c.address, mode == Mode::Floating);
                if mode == Mode::Floating {
                    hyprland::resize_to(&c.address, size.0, size.1);
                    hyprland::center(&c.address);
                }
            }
        }
        self.win.redraw();
    }

    /// Keyboard shortcut for the whole window: `win.shortcut("Ctrl+Q", || app::quit())`.
    pub fn shortcut(&mut self, spec: &str, f: impl FnMut() + 'static) {
        let keys = crate::keys::parse(spec).unwrap_or_else(|e| panic!("{e}"));
        self.shortcuts.borrow_mut().push((keys, Box::new(f)));
    }

    /// Shows the window. In floating mode on Hyprland the window is mapped
    /// with a fixed size so Hyprland floats it at the requested size
    /// (smaller first buffers), then the size range is relaxed.
    pub fn show(&mut self) {
        if self.mode.get() == Mode::Floating && hyprland::available() && !self.win.shown() {
            // Map normally, then float it as soon as Hyprland lists the window.
            // (A fixed size range at map time makes Hyprland float it too, but
            // the window then stays fixed-size for good: it cannot be tiled or
            // resized afterwards, not even by IPC.)
            let size = (self.win.w(), self.win.h());
            self.win.show();
            let title = self.win.label();
            let tries = Cell::new(0u32);
            app::add_timeout3(0.02, move |h| {
                if let Some(c) = hyprland::own_clients(Some(&title)).into_iter().next() {
                    if !c.floating {
                        hyprland::set_floating(&c.address, true);
                    }
                    hyprland::resize_to(&c.address, size.0, size.1);
                    hyprland::center(&c.address);
                } else if tries.replace(tries.get() + 1) < 100 {
                    app::repeat_timeout3(0.02, h);
                }
            });
            self.header.set_draggable(true);
        } else {
            self.win.show();
            if self.mode.get() == Mode::Fullscreen {
                self.win.fullscreen(true);
            } else if self.mode.get() == Mode::Floating {
                let m = self.mode.get();
                self.set_mode(m);
            }
        }
    }

    /// Renders the window contents offscreen into a PNG (works before
    /// `show()`), so tests and agents can look at the UI.
    pub fn snapshot(&mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        snapshot(&mut self.win, path.as_ref())
    }
}

/// Offscreen render of any window into a PNG.
pub fn snapshot(win: &mut DoubleWindow, path: &Path) -> std::io::Result<()> {
    use fltk::surface::ImageSurface;
    let (w, h) = (win.w(), win.h());
    // Lay out flex children for the current size even if never shown.
    let (x, y) = (win.x(), win.y());
    win.resize(x, y, w, h);
    let surf = ImageSurface::new(w, h, false);
    ImageSurface::push_current(&surf);
    theme::with(|t| paint::fill(0, 0, w, h, 0, t.palette.background));
    for i in 0..win.children() {
        if let Some(c) = win.child(i) {
            if c.visible() {
                surf.draw(&c, c.x(), c.y());
            }
        }
    }
    let img = surf.image();
    ImageSurface::pop_current();
    let img = img.ok_or_else(|| std::io::Error::other("offscreen render failed"))?;
    let data = img.to_rgb_data();
    png::write(path, img.data_w() as u32, img.data_h() as u32, img.depth() as u8, &data)
}

/// Draws the window's children over a freshly painted background.
///
/// Partial redraws (only some children damaged) are clipped by FLTK to the
/// damaged area, which this window's background has just covered. Every
/// child that overlaps a damaged one is drawn again, in stacking order: the
/// content under an open menu, toast or dialog, and the layer above a
/// control that changed. Otherwise a hovered menu would hide the window
/// behind it, or a redrawn control would cover a notification.
fn draw_stack(w: &mut DoubleWindow) {
    if w.damage_type() != Damage::Child {
        w.draw_children(); // full redraw: everything is damaged
        return;
    }
    let kids: Vec<fltk::widget::Widget> = (0..w.children()).filter_map(|i| w.child(i)).filter(|c| c.visible()).collect();
    let rect = |c: &fltk::widget::Widget| (c.x(), c.y(), c.x() + c.w(), c.y() + c.h());
    let damaged: Vec<(i32, i32, i32, i32)> = kids.iter().filter(|c| c.damage()).map(rect).collect();
    let overlaps = |r: (i32, i32, i32, i32)| damaged.iter().any(|d| r.0 < d.2 && d.0 < r.2 && r.1 < d.3 && d.1 < r.3);
    for c in kids {
        let mut c = c;
        if overlaps(rect(&c)) {
            w.draw_child(&mut c);
        }
    }
}

// ================================================================== Header

/// Omarchy window header: bold title on the left, action buttons on the
/// right, hairline below. Dragging it moves a floating window.
#[derive(Clone)]
pub struct Header {
    inner: Flex,
    title: Rc<RefCell<String>>,
    actions: Flex,
    draggable: Rc<Cell<bool>>,
}
fltk::widget_extends!(Header, Flex, inner);

impl Header {
    fn new(title: &str) -> Header {
        let t = theme::current();
        let h = t.spacing.control_height + 2 * t.spacing.md + 1;
        let mut inner = Flex::default().row().with_size(400, h);
        inner.set_margins(t.spacing.panel_padding - t.spacing.md, t.spacing.md, t.spacing.md, t.spacing.md + 1);
        inner.set_spacing(t.spacing.sm);
        let title_rc = Rc::new(RefCell::new(title.to_string()));
        let draggable = Rc::new(Cell::new(false));
        let mut handle = Widget::default();
        handle.set_frame(FrameType::NoBox);
        let tt = title_rc.clone();
        handle.draw(move |w| {
            theme::with(|t| paint::text_elided(&tt.borrow(), w.x(), w.y(), w.w(), w.h(), t.font.title, true, t.palette.foreground, Align::Left));
        });
        let (dr, tt) = (draggable.clone(), title_rc.clone());
        handle.handle(move |w, ev| match ev {
            Event::Push if dr.get() => {
                // Native compositor move first (any Wayland compositor), Hyprland IPC otherwise.
                if !crate::native::begin_move(w) {
                    let title = w.window().map(|win| win.label()).unwrap_or_else(|| tt.borrow().clone());
                    drag_begin(&title, 0);
                }
                true
            }
            Event::Drag if dr.get() => { drag_update(); true }
            Event::Released if dr.get() => { drag_end(); true }
            _ => false,
        });
        let mut actions = Flex::default().row();
        actions.set_spacing(t.spacing.xs);
        actions.end();
        inner.fixed(&actions, 0);
        inner.end();
        inner.draw(move |f| {
            theme::with(|t| {
                paint::fill(f.x(), f.y(), f.w(), f.h(), 0, t.palette.background);
                paint::hairline(f.x(), f.y() + f.h() - 1, f.w(), t.palette.foreground, t.palette.background, 0.12);
            });
            f.draw_children();
        });
        // cfltk runs FLTK's own draw before this closure by default: it would
        // draw damaged children first and this background would then cover them.
        inner.super_draw(false);
        Header { inner, title: title_rc, actions, draggable }
    }

    pub fn set_title(&mut self, title: &str) {
        *self.title.borrow_mut() = title.into();
        self.inner.redraw();
    }

    pub fn title(&self) -> String {
        self.title.borrow().clone()
    }

    /// Adds an action button (Nerd glyph + tooltip) on the right side.
    pub fn action(&mut self, glyph: &str, tooltip: &str, f: impl FnMut() + 'static) -> ActionButton {
        self.add(ActionButton::new(glyph, tooltip).on_click(f))
    }

    /// Adds a destructive action (hover in the urgent color).
    pub fn destructive_action(&mut self, glyph: &str, tooltip: &str, f: impl FnMut() + 'static) -> ActionButton {
        self.add(ActionButton::destructive(glyph, tooltip).on_click(f))
    }

    fn add(&mut self, b: ActionButton) -> ActionButton {
        let prev = fltk::group::Group::try_current();
        let size = b.w(); // before add(): Flex relayouts its children on add
        self.actions.add(&*b);
        self.actions.fixed(&*b, size);
        let n = self.actions.children();
        let total = n * size + (n - 1) * self.actions.spacing();
        self.inner.fixed(&self.actions, total);
        self.inner.layout();
        self.actions.layout();
        if let Some(g) = prev {
            g.begin();
        }
        b
    }

    fn set_draggable(&self, on: bool) {
        self.draggable.set(on);
    }

    #[allow(dead_code)]
    fn title_width(&self) -> i32 {
        theme::with(|t| measure_offscreen(&self.title.borrow(), t.font.title))
    }
}
