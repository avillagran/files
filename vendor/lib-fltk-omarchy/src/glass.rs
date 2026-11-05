//! Omarchy's window transparency (Appearance > Windows > Window Opacity):
//! opaque, transparent or **glassy**, followed live.
//!
//! Under Glassy windows Omarchy turns on Hyprland's blur and gives terminals
//! a see-through background (`omarchy-theme-set-glass`, opacity 0.65 unless
//! the theme sets its own), so whatever is behind the window shows through
//! blurred while the text stays crisp. This module does the same for an FLTK
//! window: the surfaces an app paints with [`fill`]/[`fill_rect`] get the
//! system's glass opacity, and text, glyphs and images drawn over them stay
//! opaque. The compositor does the blur, so no GPU feature is needed here.
//!
//! How it works on Wayland: FLTK draws into ARGB8888 buffers but tells the
//! compositor every rectangular window is opaque (`wl_surface` opaque
//! region), which stops Hyprland from showing or blurring what is behind. A
//! glass fill clears that region for the window being drawn and replaces the
//! pixels with the translucent colour (Cairo `SOURCE`), so repainting a
//! region never stacks alpha. Leaving glassy restores the opaque region.
//!
//! Elsewhere (X11/XWayland, whose FLTK windows have no alpha visual,
//! offscreen/image surfaces, which are RGB24, and macOS/Windows, see
//! docs/PLATFORMS.md) the fills stay opaque, exactly as before. Transparent, Omarchy's default, makes only the apps Omarchy
//! lists see-through through compositor rules, so it is opaque here too.
//!
//! `OMARCHY_FLTK_GLASS` overrides the system choice: `off`, `on` (the
//! system's glass opacity) or an opacity such as `0.8`. Only call the fills
//! inside a draw callback.

use crate::platform;
use crate::theme::{self, Rgb};
use fltk::draw;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};

/// Omarchy's window transparency choice (`omarchy-hyprland-window-transparency`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    Opaque,
    /// Omarchy's default.
    #[default]
    Transparent,
    Glassy,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Opaque => "opaque",
            Mode::Transparent => "transparent",
            Mode::Glassy => "glassy",
        }
    }
}

/// Opacity Omarchy gives terminal backgrounds under Glassy windows.
pub const SYSTEM_GLASS_OPACITY: f64 = 0.65;
/// Below this text over a busy background is no longer readable.
pub const MIN_OPACITY: f64 = 0.3;

/// What the system asks for, before the backend is considered.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Setting {
    pub mode: Mode,
    /// Opacity of glass surfaces (1.0 unless glassy).
    pub opacity: f64,
    /// True when `OMARCHY_FLTK_GLASS` decided instead of the system.
    pub overridden: bool,
}

impl Default for Setting {
    fn default() -> Self {
        Setting { mode: Mode::Transparent, opacity: 1.0, overridden: false }
    }
}

impl Setting {
    pub fn glassy(&self) -> bool {
        self.mode == Mode::Glassy && self.opacity < 1.0
    }
}

/// Where Omarchy keeps the transparency toggles
/// (`$XDG_STATE_HOME/omarchy/toggles/hypr`).
pub fn toggles_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("OMARCHY_FLTK_TOGGLES_DIR") {
        return PathBuf::from(p);
    }
    platform::dirs::state().join("omarchy/toggles/hypr")
}

/// The mode the toggle files say, as `omarchy-hyprland-window-transparency` reads it.
pub fn mode_in(dir: &Path) -> Mode {
    if dir.join("window-opaque.lua").is_file() {
        Mode::Opaque
    } else if dir.join("window-glassy.lua").is_file() {
        Mode::Glassy
    } else {
        Mode::Transparent
    }
}

/// The glass opacity of `theme_dir`: the `[window] opacity` Omarchy writes
/// into the theme's alacritty.toml under Glassy windows (a theme's own
/// value wins there), else Omarchy's 0.65.
pub fn opacity_in(theme_dir: &Path) -> f64 {
    let text = std::fs::read_to_string(theme_dir.join("alacritty.toml")).unwrap_or_default();
    let value = crate::toml::parse(&text).get("window.opacity").and_then(|v| v.as_f64());
    clamp(value.unwrap_or(SYSTEM_GLASS_OPACITY))
}

fn clamp(v: f64) -> f64 {
    if v.is_finite() { v.clamp(MIN_OPACITY, 1.0) } else { SYSTEM_GLASS_OPACITY }
}

/// Reads the system choice now (and the override).
pub fn read_setting() -> Setting {
    let theme_dir = theme::theme_dir();
    let system = || {
        let mode = mode_in(&toggles_dir());
        let opacity = if mode == Mode::Glassy { opacity_in(&theme_dir) } else { 1.0 };
        Setting { mode, opacity, overridden: false }
    };
    match std::env::var("OMARCHY_FLTK_GLASS").ok().as_deref().map(str::trim) {
        None | Some("") | Some("system") => system(),
        Some("off") | Some("0") | Some("opaque") => Setting { mode: Mode::Opaque, opacity: 1.0, overridden: true },
        Some("on") | Some("1") | Some("glassy") => Setting { mode: Mode::Glassy, opacity: opacity_in(&theme_dir), overridden: true },
        Some(v) => match v.parse::<f64>() {
            Ok(a) => Setting { mode: Mode::Glassy, opacity: clamp(a), overridden: true },
            Err(_) => system(),
        },
    }
}

type Listener = Box<dyn FnMut(&Setting)>;

thread_local! {
    static CURRENT: Cell<Option<Setting>> = const { Cell::new(None) };
    static LISTENERS: RefCell<Vec<Listener>> = const { RefCell::new(Vec::new()) };
    /// Windows whose opaque region a glass fill cleared: (Fl_Window*, wl_surface*).
    static CLEARED: RefCell<Vec<(usize, usize)>> = const { RefCell::new(Vec::new()) };
    static STATS: Cell<Stats> = const { Cell::new(Stats { glass_fills: 0, opaque_fills: 0, regions_cleared: 0, regions_restored: 0 }) };
}

/// Counters for diagnostics and tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    pub glass_fills: u64,
    pub opaque_fills: u64,
    pub regions_cleared: u64,
    pub regions_restored: u64,
}

pub fn stats() -> Stats {
    STATS.with(|s| s.get())
}

fn bump(f: impl FnOnce(&mut Stats)) {
    STATS.with(|s| {
        let mut v = s.get();
        f(&mut v);
        s.set(v);
    });
}

/// The active setting (read on first use).
pub fn setting() -> Setting {
    CURRENT.with(|c| {
        if let Some(s) = c.get() {
            return s;
        }
        let s = read_setting();
        c.set(Some(s));
        s
    })
}

pub fn mode() -> Mode {
    setting().mode
}

/// The glass opacity this process paints with now: Some only when the
/// system is glassy and the backend can show it (native Wayland).
pub fn opacity() -> Option<f64> {
    let s = setting();
    (s.glassy() && wayland()).then_some(s.opacity)
}

/// True when glass fills are see-through now.
pub fn active() -> bool {
    opacity().is_some()
}

/// Called after every change of the setting (UI thread, for the program's life).
pub fn on_change(f: impl FnMut(&Setting) + 'static) {
    LISTENERS.with(|l| l.borrow_mut().push(Box::new(f)));
}

/// Re-reads the system choice; on change restyles every window and returns
/// true. [`crate::app::ThemeWatcher`] calls it when the toggles change.
pub fn reload() -> bool {
    let fresh = read_setting();
    set(fresh)
}

/// Makes `s` the active setting (tests, apps with their own preference).
pub fn set(s: Setting) -> bool {
    let old = CURRENT.with(|c| c.replace(Some(s)));
    if old == Some(s) {
        return false;
    }
    if !(s.glassy() && wayland()) {
        restore_regions();
    }
    let mut list = LISTENERS.with(|l| std::mem::take(&mut *l.borrow_mut()));
    for f in list.iter_mut() {
        f(&s);
    }
    LISTENERS.with(|l| {
        let mut l = l.borrow_mut();
        let added = std::mem::take(&mut *l);
        *l = list;
        l.extend(added);
    });
    fltk::app::redraw();
    true
}

/// Surface fill: rectangle, rounded when `radius > 0`, with the glass
/// opacity when glass is active for the window being drawn, else opaque
/// (same pixels as [`crate::paint::fill`]).
pub fn fill(x: i32, y: i32, w: i32, h: i32, radius: i32, color: Rgb) {
    if w <= 0 || h <= 0 {
        return;
    }
    let r = radius.min(w / 2).min(h / 2);
    surface(color, || if r > 0 { draw::draw_rounded_rectf(x, y, w, h, r) } else { draw::draw_rectf(x, y, w, h) });
}

/// Runs `shape` (FLTK filled shapes: `draw_rectf`, `draw_rounded_rectf`,
/// polygons, pies) as a glass surface of `color`: the draw colour is set
/// first, and while glass is active the shape replaces the pixels with
/// `color` at the glass opacity. Otherwise it is an ordinary opaque fill.
/// Do not draw text or images inside `shape`.
pub fn surface(color: Rgb, shape: impl FnOnce()) {
    draw::set_draw_color(color.fl());
    match glass_target() {
        Some((t, a)) => {
            let c = |v: u8| f64::from(v) / 255.0;
            platform::glass::begin(&t, c(color.r()), c(color.g()), c(color.b()), a);
            shape();
            platform::glass::end(&t);
            bump(|s| s.glass_fills += 1);
        }
        None => {
            shape();
            bump(|s| s.opaque_fills += 1);
        }
    }
}

/// [`fill`] without corners.
pub fn fill_rect(x: i32, y: i32, w: i32, h: i32, color: Rgb) {
    fill(x, y, w, h, 0, color);
}

/// The drawing target and opacity when the current drawing goes to a shown
/// Wayland window buffer (ARGB32) and glass is active. Clears that window's
/// opaque region the first time (and after the window gets a new surface).
fn glass_target() -> Option<(platform::glass::Target, f64)> {
    let a = opacity()?;
    let t = platform::glass::target()?;
    let key = (t.win, t.surface);
    let fresh = CLEARED.with(|c| {
        let mut c = c.borrow_mut();
        if c.contains(&key) {
            return false;
        }
        c.retain(|(w, _)| *w != key.0);
        c.push(key);
        true
    });
    if fresh {
        platform::glass::set_opaque_region(t.surface, false);
        bump(|s| s.regions_cleared += 1);
    }
    Some((t, a))
}

/// Gives every still-shown window whose region a glass fill cleared its
/// full opaque region back (glass turned off).
fn restore_regions() {
    let cleared = CLEARED.with(|c| std::mem::take(&mut *c.borrow_mut()));
    if cleared.is_empty() || !wayland() {
        return;
    }
    let live: Vec<usize> = fltk::app::windows()
        .unwrap_or_default()
        .iter()
        .map(|w| { use fltk::prelude::WidgetExt; w.as_widget_ptr() as usize })
        .collect();
    for (win, surface) in cleared {
        if !live.contains(&win) {
            continue; // deleted: its surface went with it
        }
        // `win` is in FLTK's live window list; its current surface is looked
        // up again, so a hidden window is skipped and a re-shown one (FLTK
        // already made the new surface opaque) is left alone.
        if platform::glass::current_surface(win) != Some(surface) {
            continue;
        }
        platform::glass::set_opaque_region(surface, true);
        bump(|s| s.regions_restored += 1);
    }
}

/// True when FLTK runs on its native Wayland backend (glass can show).
pub fn wayland() -> bool {
    platform::wayland()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("fltk-omarchy-glass-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn mode_follows_omarchy_toggle_files() {
        let d = dir("mode");
        assert_eq!(mode_in(&d), Mode::Transparent);
        assert_eq!(mode_in(&d.join("missing")), Mode::Transparent);
        std::fs::write(d.join("window-glassy.lua"), "").unwrap();
        assert_eq!(mode_in(&d), Mode::Glassy);
        // Same priority as omarchy-hyprland-window-transparency.
        std::fs::write(d.join("window-opaque.lua"), "").unwrap();
        assert_eq!(mode_in(&d), Mode::Opaque);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn opacity_is_the_theme_glass_or_omarchys() {
        let d = dir("opacity");
        assert_eq!(opacity_in(&d), SYSTEM_GLASS_OPACITY);
        std::fs::write(d.join("alacritty.toml"), "[colors.primary]\nbackground = \"#000000\"\n# omarchy-glass begin\n[window]\nopacity = 0.8\n# omarchy-glass end\n").unwrap();
        assert_eq!(opacity_in(&d), 0.8);
        std::fs::write(d.join("alacritty.toml"), "[window]\nopacity = 0.05\n").unwrap();
        assert_eq!(opacity_in(&d), MIN_OPACITY);
        std::fs::write(d.join("alacritty.toml"), "[window]\nopacity = 4\n").unwrap();
        assert_eq!(opacity_in(&d), 1.0);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn only_glassy_below_one_is_glass() {
        assert!(!Setting::default().glassy());
        assert!(Setting { mode: Mode::Glassy, opacity: 0.65, overridden: false }.glassy());
        assert!(!Setting { mode: Mode::Glassy, opacity: 1.0, overridden: false }.glassy());
        assert!(!Setting { mode: Mode::Opaque, opacity: 0.65, overridden: false }.glassy());
    }
}
