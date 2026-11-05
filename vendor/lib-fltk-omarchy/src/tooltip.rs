//! Omarchy tooltips for every widget, drawn by the toolkit.
//!
//! [`crate::App::new`] (or, in an app that starts FLTK itself, the first
//! [`crate::theme::set`]) turns FLTK's own tooltip window off and shows the
//! tooltip of whatever is under the pointer as an in-window surface, the way
//! Omarchy's shell does (`ToolTipBase`/`ToolTipSurface`, omarchy#15694):
//! the `[tooltip]` colors of the theme, its border width, `bodySmall` text
//! in the theme font, `controlPaddingX/Y` padding, the theme corner radius
//! (it follows [`crate::theme::set_corners`]), plain text wrapped at 360 px
//! (scaled), centered above the item and flipped below it when there is
//! no room, kept inside the window. It appears after 400 ms; while one is
//! up (and 400 ms after it hides) the next one appears at once. It stays
//! until the pointer leaves, a button or key is pressed or the wheel turns.
//!
//! Nothing to do in app code: any widget with a tooltip (`set_tooltip`,
//! `.tooltip(..)`, `.bind_tooltip(..)`, header actions) gets it, plain fltk
//! widgets included. A canvas that changes its tooltip while the pointer
//! moves (one tooltip per region) is followed too; on a large widget the
//! tooltip goes below the pointer instead of above the whole widget.
//!
//! The optional `~/.config/omarchy/tooltips.json` written by Omarchy's
//! tooltip settings is read live (the theme watcher, and a `stat` when a
//! tooltip shows):
//! `{"shape":"theme|square|rounded","borderMode":"theme|transparent|custom","borderColor":"#rrggbb"}`.
//! Missing keys follow the theme; an invalid file keeps the last valid
//! settings.
//!
//! Text is whatever the widget's tooltip says now: bind it with
//! `bind_tooltip(&i18n::text(..))` and a language switch relabels a shown
//! tooltip. Right to left, lines are right-aligned. A widget's tooltip is
//! also its accessible description (see [`crate::a11y`]); the surface
//! itself is hidden from assistive technology.

use crate::layout;
use crate::paint;
use crate::theme::{self, Rgb, Theme};
use fltk::{app, enums::*, prelude::*, widget::Widget};
use std::cell::RefCell;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Hover time before the first tooltip shows (Omarchy's `delay: 400`).
pub const DELAY: Duration = Duration::from_millis(400);
/// After a tooltip hides, the next one skips [`DELAY`] for this long
/// (Omarchy's bar `tooltipWarmTimer`).
pub const WARM: Duration = Duration::from_millis(400);

/// Corner shape (`tooltips.json` `shape`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shape {
    /// The theme radius ([`Theme::radius`], which follows `set_corners`).
    #[default]
    Theme,
    Square,
    /// 8 px at base size 12.
    Rounded,
}

/// Border color (`tooltips.json` `borderMode` / `borderColor`). The border
/// width always comes from the theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BorderMode {
    /// `[tooltip] border` of the theme.
    #[default]
    Theme,
    Transparent,
    Custom(Rgb),
}

/// Omarchy's tooltip preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Settings {
    pub shape: Shape,
    pub border: BorderMode,
}

impl Settings {
    /// Parses `tooltips.json`. `None` when it is not a valid settings
    /// object (the caller keeps the last valid settings); empty text is the
    /// defaults.
    pub fn parse(json: &str) -> Option<Settings> {
        if json.trim().is_empty() {
            return Some(Settings::default());
        }
        let v: serde_json::Value = serde_json::from_str(json).ok()?;
        let o = v.as_object()?;
        let field = |k: &str| -> Result<Option<&str>, ()> {
            match o.get(k) {
                None => Ok(None),
                Some(v) => v.as_str().map(Some).ok_or(()),
            }
        };
        let shape = match field("shape").ok()? {
            None | Some("theme") => Shape::Theme,
            Some("square") => Shape::Square,
            Some("rounded") => Shape::Rounded,
            Some(_) => return None,
        };
        let color = match field("borderColor").ok()? {
            None => Rgb(0xffffff),
            Some(c)
                if c.len() == 7
                    && c.starts_with('#')
                    && c[1..].chars().all(|h| h.is_ascii_hexdigit()) =>
            {
                Rgb::parse(c)?
            }
            Some(_) => return None,
        };
        let border = match field("borderMode").ok()? {
            None | Some("theme") => BorderMode::Theme,
            Some("transparent") => BorderMode::Transparent,
            Some("custom") => BorderMode::Custom(color),
            Some(_) => return None,
        };
        Some(Settings { shape, border })
    }
}

/// The shared Omarchy tooltip settings file
/// (`$XDG_CONFIG_HOME/omarchy/tooltips.json`; `OMARCHY_FLTK_TOOLTIPS`
/// overrides it).
pub fn settings_path() -> PathBuf {
    if let Some(p) = std::env::var_os("OMARCHY_FLTK_TOOLTIPS").filter(|v| !v.is_empty()) {
        return PathBuf::from(p);
    }
    crate::platform::dirs::omarchy_config().join("tooltips.json")
}

/// The settings in use.
pub fn settings() -> Settings {
    TIP.with(|t| t.borrow().settings)
}

/// Uses `s` until the next [`reload_settings`] (the theme watcher reloads
/// when `tooltips.json` changes). A shown tooltip is restyled.
pub fn set_settings(s: Settings) {
    let changed = TIP.with(|t| std::mem::replace(&mut t.borrow_mut().settings, s) != s);
    if changed {
        refresh();
    }
}

/// Re-reads [`settings_path`]: a missing file means the defaults, an
/// invalid one keeps the current settings. Returns true on change.
pub fn reload_settings() -> bool {
    let path = settings_path();
    TIP.with(|t| t.borrow_mut().settings_stamp = Some(stamp(&path)));
    let next = match std::fs::read_to_string(path) {
        Ok(text) => Settings::parse(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(Settings::default()),
        Err(_) => None,
    };
    match next {
        Some(s) if s != settings() => {
            set_settings(s);
            true
        }
        _ => false,
    }
}

/// A shown tooltip: its text and rectangle in its top window's coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub text: String,
    pub rect: (i32, i32, i32, i32),
}

/// The tooltip on screen, if any.
pub fn shown() -> Option<Shown> {
    TIP.with(|t| {
        let t = t.borrow();
        let s = t.shown.as_ref()?;
        if s.layer.was_deleted() {
            return None;
        }
        Some(Shown {
            text: s.target.text.clone(),
            rect: (s.layer.x(), s.layer.y(), s.layer.w(), s.layer.h()),
        })
    })
}

/// Shows `w`'s tooltip now, placed at the widget (for a keyboard shortcut,
/// docs and tests; hovering needs nothing). False when `w` has no tooltip
/// or no window.
pub fn show<W: WidgetExt>(w: &W) -> bool {
    // SAFETY: `w` is a live widget; the copy tracks its deletion.
    let w = unsafe { Widget::from_widget_ptr(w.as_widget_ptr()) };
    let Some((owner, text)) = tooltip_of(&w) else {
        return false;
    };
    hide_now(false);
    open(Target {
        owner,
        text,
        pointer: None,
    })
}

/// Hides the tooltip (if any).
pub fn hide() {
    hide_now(false);
}

/// Placement of a tooltip of size `(w, h)` for `anchor` (an item, or the
/// pointer as `(x, y, 1, cursor)`), inside a `win_w × win_h` window with
/// `margin` to its edges: centered above the anchor, below it when it does
/// not fit above (`below_first` tries below first). Returns `(x, y)`.
pub fn place(
    anchor: (i32, i32, i32, i32),
    size: (i32, i32),
    window: (i32, i32),
    margin: i32,
    below_first: bool,
) -> (i32, i32) {
    let (ax, ay, aw, ah) = anchor;
    let (w, h) = size;
    let (ww, wh) = window;
    let x = (ax + aw / 2 - w / 2)
        .min(ww - w - margin)
        .max(margin.min(ww - w).max(0));
    let above = ay - h - margin;
    let below = ay + ah + margin;
    let fits_above = above >= margin;
    let fits_below = below + h <= wh - margin;
    let y = match (below_first, fits_above, fits_below) {
        (false, true, _) | (true, true, false) => above,
        (_, _, true) => below,
        // Neither: the side with more room, kept inside the window.
        _ => {
            if ay > wh - (ay + ah) {
                above
            } else {
                below
            }
        }
    };
    (x, y.min(wh - h - margin).max(0))
}

/// Greedy word wrap of `text` at `max_w` pixels with the current font;
/// explicit newlines are kept, a word wider than a line is cut.
pub fn wrap(text: &str, max_w: i32) -> Vec<String> {
    let fits = |s: &str| fltk::draw::width(s).ceil() as i32 <= max_w;
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if fits(&candidate) || (line.is_empty() && word.is_empty()) {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                out.push(std::mem::take(&mut line));
            }
            // The word alone: cut it where it overflows.
            let mut piece = String::new();
            for c in word.chars() {
                piece.push(c);
                if !fits(&piece) && piece.chars().count() > 1 {
                    piece.pop();
                    out.push(std::mem::take(&mut piece));
                    piece.push(c);
                }
            }
            line = piece;
        }
        out.push(line);
    }
    out
}

// ---------------------------------------------------------------- state

#[derive(Clone)]
struct Target {
    /// The widget whose tooltip it is.
    owner: Widget,
    text: String,
    /// Pointer in top-window coordinates when it was hovered: on a wide
    /// widget the tooltip is centered on it, on a tall one it goes below it.
    pointer: Option<(i32, i32)>,
}

impl Target {
    fn same(&self, other: &Target) -> bool {
        self.owner.as_widget_ptr() == other.owner.as_widget_ptr() && self.text == other.text
    }
}

struct ShownTip {
    layer: Widget,
    target: Target,
    window_size: (i32, i32),
}

#[derive(Default)]
struct Tip {
    installed: bool,
    settings: Settings,
    /// `tooltips.json` as last read (modified time, length).
    settings_stamp: Option<Stamp>,
    pending: Option<(Target, app::TimeoutHandle)>,
    shown: Option<ShownTip>,
    warm_until: Option<Instant>,
    /// Clicked or typed on: no tooltip again until the pointer leaves it.
    suppressed: Option<(usize, String)>,
}

thread_local! {
    static TIP: RefCell<Tip> = RefCell::new(Tip::default());
}

type Stamp = Option<(std::time::SystemTime, u64)>;

fn stamp(path: &std::path::Path) -> Stamp {
    std::fs::metadata(path)
        .ok()
        .map(|m| (m.modified().unwrap_or(std::time::UNIX_EPOCH), m.len()))
}

/// Re-reads `tooltips.json` if it changed since it was read (one `stat`
/// per shown tooltip): live in apps without the toolkit's theme watcher.
fn refresh_settings() {
    let path = settings_path();
    if TIP.with(|t| t.borrow().settings_stamp.as_ref() != Some(&stamp(&path))) {
        reload_settings();
    }
}

/// Turns FLTK's tooltip window off and starts following the pointer.
/// Called by [`crate::App::new`] and by the first [`theme::set`] on the
/// UI thread, so apps that start FLTK themselves (`fltk::app::App`) and
/// only apply the Omarchy theme get the same tooltips. Events come from
/// [`crate::focus`]'s dispatcher, or from a pass-through one when the
/// keyboard router is not installed. Idempotent.
pub(crate) fn install() {
    if TIP.with(|t| std::mem::replace(&mut t.borrow_mut().installed, true)) {
        return;
    }
    fltk::misc::Tooltip::disable();
    if !crate::focus::installed() {
        // SAFETY: `pass_through` hands every event to fltk's own handler
        // with the same window pointer, unchanged.
        unsafe { app::event_dispatch(pass_through) };
    }
    reload_settings();
    theme::on_change(|_| refresh());
    // A shown tooltip whose widget went away, was hidden, relabeled (a
    // language switch, `bind_tooltip`) or whose window was resized.
    app::add_check(|_| check_shown());
}

/// [`install`] from [`theme::set`]: only once FLTK runs, on its thread.
pub(crate) fn auto_install() {
    if !installed() && app::is_initialized() && app::is_ui_thread() {
        install();
    }
}

fn pass_through(ev: Event, win: app::WindowPtr) -> bool {
    // SAFETY: `win` comes from fltk for this very event.
    let used = unsafe { app::handle_raw(ev, win) };
    track(ev, win);
    used
}

/// Whether the toolkit draws tooltips (after [`crate::App::new`] or the
/// first [`theme::set`]).
pub fn installed() -> bool {
    TIP.with(|t| t.borrow().installed)
}

/// Called by the event dispatcher after FLTK handled `ev`, which went to
/// `win` (its event coordinates are `win`'s).
pub(crate) fn track(ev: Event, win: app::WindowPtr) {
    if !installed() {
        return;
    }
    if win.is_null() {
        return;
    }
    // SAFETY: fltk passed this live window pointer for the event (a
    // Fl_Window is a Fl_Widget).
    let win = unsafe { Widget::from_widget_ptr(win as _) };
    match ev {
        Event::Push | Event::KeyDown | Event::MouseWheel | Event::DndEnter => {
            let hovered = hover_target(&win);
            TIP.with(|t| {
                t.borrow_mut().suppressed =
                    hovered.map(|h| (h.owner.as_widget_ptr() as usize, h.text));
            });
            cancel_pending();
            hide_now(false);
        }
        Event::Move
        | Event::Enter
        | Event::Leave
        | Event::Released
        | Event::Hide
        | Event::Deactivate => hover(&win),
        _ => {}
    }
}

fn hover(win: &Widget) {
    let Some(target) = hover_target(win) else {
        let shown = TIP.with(|t| t.borrow().shown.is_some());
        cancel_pending();
        TIP.with(|t| t.borrow_mut().suppressed = None);
        hide_now(shown);
        return;
    };
    let key = (target.owner.as_widget_ptr() as usize, target.text.clone());
    let (same_shown, same_pending, suppressed) = TIP.with(|t| {
        let t = t.borrow();
        (
            t.shown.as_ref().is_some_and(|s| s.target.same(&target)),
            t.pending.as_ref().is_some_and(|(p, _)| p.same(&target)),
            t.suppressed.as_ref() == Some(&key),
        )
    });
    if same_shown || same_pending {
        return;
    }
    if suppressed {
        cancel_pending();
        hide_now(false);
        return;
    }
    TIP.with(|t| t.borrow_mut().suppressed = None);
    cancel_pending();
    let warm = TIP.with(|t| {
        let t = t.borrow();
        t.shown.is_some() || t.warm_until.is_some_and(|w| Instant::now() < w)
    });
    hide_now(false);
    if warm {
        open(target);
        return;
    }
    let p = target.clone();
    let handle = app::add_timeout3(DELAY.as_secs_f64(), move |_| {
        let still = TIP.with(|t| {
            t.borrow_mut()
                .pending
                .take_if(|(q, _)| q.same(&p))
                .is_some()
        });
        if still && !p.owner.was_deleted() && p.owner.visible() {
            open(p.clone());
        }
    });
    TIP.with(|t| t.borrow_mut().pending = Some((target, handle)));
}

fn cancel_pending() {
    if let Some((_, h)) = TIP.with(|t| t.borrow_mut().pending.take()) {
        app::remove_timeout3(h);
    }
}

/// The tooltip of the widget under the pointer, if any; the event went to
/// `win`.
fn hover_target(win: &Widget) -> Option<Target> {
    if app::grab().is_some() || win.was_deleted() {
        return None;
    }
    let below: Widget = app::belowmouse()?;
    let top = below.top_window().or_else(|| below.as_window())?;
    let origin = window_origin(&*win.as_window()?, &*top);
    let (px, py) = (app::event_x() + origin.0, app::event_y() + origin.1);
    // Descend from the widget FLTK has under the pointer to the deepest
    // visible child there: a Frame or a Flex child that does not take
    // FL_ENTER still has its own tooltip. Subwindows are not entered (FLTK
    // already puts the pointer inside them when it is there).
    let layer = TIP.with(|t| t.borrow().shown.as_ref().map(|s| s.layer.as_widget_ptr()));
    let mut w = below;
    'down: while let Some(g) = w.as_group() {
        let (ox, oy) = child_origin(&w, &*top);
        for i in (0..g.children()).rev() {
            let Some(c) = g.child(i) else { continue };
            if !c.visible() || c.as_window().is_some() || Some(c.as_widget_ptr()) == layer {
                continue;
            }
            let (cx, cy) = (c.x() + ox, c.y() + oy);
            if px >= cx && px < cx + c.w() && py >= cy && py < cy + c.h() {
                w = c;
                continue 'down;
            }
        }
        break;
    }
    let (owner, text) = tooltip_of(&w)?;
    let (ox, oy, ow, oh) = rect_in(&owner, &*top);
    let inside = px >= ox && px < ox + ow && py >= oy && py < oy + oh;
    Some(Target {
        owner,
        text,
        pointer: inside.then_some((px, py)),
    })
}

/// The widget (or nearest parent) with a tooltip and its text; an empty
/// tooltip stops the search (FLTK: `""` hides the parent's tooltip).
fn tooltip_of(w: &Widget) -> Option<(Widget, String)> {
    let mut o = Some(w.clone());
    while let Some(x) = o {
        if let Some(t) = x.tooltip() {
            return (!t.is_empty()).then_some((x, t));
        }
        o = x.parent().map(|p| p.as_base_widget());
    }
    None
}

fn same<A: WidgetExt + ?Sized, B: WidgetExt + ?Sized>(a: &A, b: &B) -> bool {
    a.as_widget_ptr() == b.as_widget_ptr()
}

/// Origin of `win`'s coordinates in `top`'s coordinates.
fn window_origin(win: &dyn WindowExt, top: &dyn WindowExt) -> (i32, i32) {
    if same(win, top) {
        (0, 0)
    } else {
        (win.x_root() - top.x_root(), win.y_root() - top.y_root())
    }
}

/// Origin of the coordinates `w`'s children use, in `top`'s coordinates.
fn child_origin(w: &Widget, top: &dyn WindowExt) -> (i32, i32) {
    match w.as_window().or_else(|| w.window()) {
        Some(win) => window_origin(&*win, top),
        None => (0, 0),
    }
}

/// `w`'s rectangle in `top`'s coordinates.
fn rect_in(w: &Widget, top: &dyn WindowExt) -> (i32, i32, i32, i32) {
    if let Some(win) = w.as_window() {
        let (x, y) = window_origin(&*win, top);
        return (x, y, w.w(), w.h());
    }
    let (dx, dy) = w.window().map_or((0, 0), |win| window_origin(&*win, top));
    (w.x() + dx, w.y() + dy, w.w(), w.h())
}

// ---------------------------------------------------------------- surface

fn scaled(t: &Theme, px: f64) -> i32 {
    (px * t.font.base as f64 / 12.0).round() as i32
}

fn max_width(t: &Theme) -> i32 {
    scaled(t, 360.0)
}

struct Look {
    fill: Rgb,
    text: Rgb,
    border: Option<Rgb>,
    border_width: i32,
    radius: i32,
}

fn look(t: &Theme, s: Settings) -> Look {
    let tip = &t.tooltip;
    let fill = tip.fill(t.palette.background);
    let border = match s.border {
        BorderMode::Theme => Some(tip.border.over(fill, tip.border_alpha)),
        BorderMode::Transparent => None,
        BorderMode::Custom(c) => Some(c),
    };
    let radius = match s.shape {
        Shape::Theme => t.radius,
        Shape::Square => 0,
        Shape::Rounded => scaled(t, 8.0),
    };
    Look {
        fill,
        text: tip.text,
        border,
        border_width: tip.border_width.max(0),
        radius,
    }
}

/// Wrapped lines and the surface size for `text` in a window `win_w` wide.
fn measure(t: &Theme, text: &str, win_w: i32) -> (Vec<String>, i32, i32, i32) {
    let margin = t.spacing.sm;
    let inset_x = t.spacing.control_padding_x + t.tooltip.border_width.max(0);
    let inset_y = t.spacing.control_padding_y + t.tooltip.border_width.max(0);
    let max_w = max_width(t).min(win_w - 2 * margin).max(2 * inset_x + 1);
    fltk::draw::set_font(paint::font(false), t.font.body_small);
    let lines = wrap(text, max_w - 2 * inset_x);
    let line_h = fltk::draw::height().max(t.font.body_small);
    let text_w = lines
        .iter()
        .map(|l| fltk::draw::width(l).ceil() as i32)
        .max()
        .unwrap_or(0);
    let w = (text_w + 2 * inset_x).min(max_w);
    let h = lines.len() as i32 * line_h + 2 * inset_y;
    (lines, line_h, w, h)
}

/// Rectangle of a target's tooltip in its top window.
fn layout_for(target: &Target) -> Option<(i32, i32, i32, i32)> {
    let top = target
        .owner
        .top_window()
        .or_else(|| target.owner.as_window())?;
    let (ww, wh) = (top.w(), top.h());
    let owner = rect_in(&target.owner, &*top);
    Some(theme::with(|t| {
        let (_, _, w, h) = measure(t, &target.text, ww);
        let margin = t.spacing.sm;
        let wide = owner.2 > max_width(t);
        let tall = owner.3 > 2 * t.spacing.control_height;
        let (x, y) = match target.pointer {
            // A canvas or a large area: below the pointer (clear of the
            // cursor), above it when there is no room.
            Some((px, py)) if tall => {
                place((px, py, 1, scaled(t, 20.0)), (w, h), (ww, wh), margin, true)
            }
            // A wide row: above (or below) it, at the pointer.
            Some((px, _)) if wide => {
                place((px, owner.1, 1, owner.3), (w, h), (ww, wh), margin, false)
            }
            _ => place(owner, (w, h), (ww, wh), margin, false),
        };
        (x, y, w, h)
    }))
}

fn open(target: Target) -> bool {
    refresh_settings();
    let Some(rect) = layout_for(&target) else {
        return false;
    };
    let Some(top) = target.owner.top_window() else {
        return false;
    };
    let Some(mut layer) = crate::overlay::open_layer(&target.owner, Some(rect), false) else {
        return false;
    };
    // Never under the pointer for FLTK: events go to what is below it.
    layer.deactivate();
    layer.handle(|_, _| false);
    let owner = target.owner.clone();
    layer.draw(move |l| draw(l, &owner));
    crate::a11y::describe(&layer, |n| {
        n.hidden(true);
    });
    TIP.with(|t| {
        let mut t = t.borrow_mut();
        t.warm_until = None;
        t.shown = Some(ShownTip {
            layer,
            target,
            window_size: (top.w(), top.h()),
        });
    });
    true
}

fn draw(l: &Widget, owner: &Widget) {
    let text = TIP
        .with(|t| t.borrow().shown.as_ref().map(|s| s.target.text.clone()))
        .unwrap_or_default();
    let s = settings();
    let win_w = l.top_window().map_or(l.w(), |w| w.w());
    theme::with(|t| {
        let lk = look(t, s);
        let (x, y, w, h) = (l.x(), l.y(), l.w(), l.h());
        paint::surface(
            x,
            y,
            w,
            h,
            lk.radius,
            Some(lk.fill),
            lk.border.map(|c| (c, lk.border_width)),
        );
        let (lines, line_h, _, _) = measure(t, &text, win_w);
        let inset_x = t.spacing.control_padding_x + lk.border_width;
        let inset_y = t.spacing.control_padding_y + lk.border_width;
        let m = layout::Mirror::of(owner);
        let align = m.align(Align::Left);
        let tw = w - 2 * inset_x;
        for (i, line) in lines.iter().enumerate() {
            paint::text(
                line,
                x + inset_x,
                y + inset_y + i as i32 * line_h,
                tw,
                line_h,
                t.font.body_small,
                false,
                lk.text,
                align,
            );
        }
    });
}

/// Hides the shown tooltip; `warm` lets the next one skip the delay.
fn hide_now(warm: bool) {
    let Some(s) = TIP.with(|t| t.borrow_mut().shown.take()) else {
        return;
    };
    if warm {
        TIP.with(|t| t.borrow_mut().warm_until = Some(Instant::now() + WARM));
    }
    if !s.layer.was_deleted() {
        crate::overlay::close_layer(&s.layer);
    }
}

/// Re-lays out a shown tooltip (theme, settings or text changed).
fn refresh() {
    let Some(target) = TIP.with(|t| t.borrow().shown.as_ref().map(|s| s.target.clone())) else {
        return;
    };
    hide_now(false);
    if !target.owner.was_deleted() {
        open(target);
    }
}

fn check_shown() {
    let Some((target, layer, size)) = TIP.with(|t| {
        t.borrow()
            .shown
            .as_ref()
            .map(|s| (s.target.clone(), s.layer.clone(), s.window_size))
    }) else {
        return;
    };
    if layer.was_deleted() || layer.parent().is_none() {
        TIP.with(|t| t.borrow_mut().shown = None);
        return;
    }
    if target.owner.was_deleted() || !target.owner.visible() {
        hide_now(false);
        return;
    }
    let now = target.owner.top_window().map(|w| (w.w(), w.h()));
    if now != Some(size) {
        hide_now(false);
        return;
    }
    match target.owner.tooltip() {
        Some(t) if t == target.text => {}
        Some(t) if !t.is_empty() => {
            TIP.with(|s| {
                if let Some(s) = s.borrow_mut().shown.as_mut() {
                    s.target.text = t;
                }
            });
            refresh();
        }
        _ => hide_now(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_follow_omarchy_tooltip_preferences() {
        assert_eq!(Settings::parse(""), Some(Settings::default()));
        assert_eq!(Settings::parse("{}"), Some(Settings::default()));
        let s = Settings::parse(
            r##"{"shape":"rounded","borderMode":"custom","borderColor":"#FF8800","extra":1}"##,
        )
        .unwrap();
        assert_eq!(
            s,
            Settings {
                shape: Shape::Rounded,
                border: BorderMode::Custom(Rgb(0xff8800))
            }
        );
        assert_eq!(
            Settings::parse(r#"{"shape":"square","borderMode":"transparent"}"#).unwrap(),
            Settings {
                shape: Shape::Square,
                border: BorderMode::Transparent
            }
        );
        // Invalid values or files keep the last valid settings (None).
        assert_eq!(Settings::parse(r#"{"shape":"blob"}"#), None);
        assert_eq!(Settings::parse(r##"{"borderColor":"#fff"}"##), None);
        assert_eq!(Settings::parse(r#"{"shape":3}"#), None);
        assert_eq!(Settings::parse("[1]"), None);
        assert_eq!(Settings::parse("{\"shape\":"), None);
    }

    #[test]
    fn placement_centers_above_flips_below_and_stays_inside() {
        // Room above: centered, `margin` above the item.
        assert_eq!(
            place((100, 100, 40, 28), (80, 27), (400, 300), 4, false),
            (80, 69)
        );
        // A header action at the top: below it.
        assert_eq!(
            place((100, 4, 40, 28), (80, 27), (400, 300), 4, false),
            (80, 36)
        );
        // Kept inside the window at the right edge and the left edge.
        assert_eq!(
            place((380, 100, 20, 28), (80, 27), (400, 300), 4, false).0,
            316
        );
        assert_eq!(place((0, 100, 20, 28), (80, 27), (400, 300), 4, false).0, 4);
        // Pointer anchor: below first, above at the bottom edge.
        assert_eq!(
            place((200, 100, 1, 20), (80, 27), (400, 300), 4, true),
            (160, 124)
        );
        assert_eq!(
            place((200, 280, 1, 20), (80, 27), (400, 300), 4, true),
            (160, 249)
        );
        // Wider than the window: starts at 0.
        assert_eq!(
            place((10, 100, 20, 28), (500, 27), (400, 300), 4, false).0,
            0
        );
    }
}
