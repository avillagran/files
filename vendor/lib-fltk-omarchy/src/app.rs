//! Application runtime: fltk init on the UI thread, Omarchy theme applied
//! and watched live (inotify on Linux, polling elsewhere), Omarchy tooltips ([`crate::tooltip`]), and the event loop
//! that also runs work posted from background tasks.

use crate::{task, theme};
use fltk::{app, misc::Tooltip};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Flush pending paints, preserving open overlays on custom FLTK windows.
/// Use this when flushing explicitly outside the normal toolkit event loop.
pub fn flush() {
    crate::overlay::prepare_redraw();
    app::flush();
}

/// `let app = fltk_omarchy::App::new("dev.me.notes");` … `app.run();`
pub struct App {
    id: String,
    _fltk: app::App,
    watcher: Option<ThemeWatcher>,
}

impl App {
    /// Initializes fltk on this thread (the UI thread), loads and applies
    /// the Omarchy theme and starts following theme changes.
    /// `id` is a reverse-DNS app id; it becomes the Wayland app id.
    pub fn new(id: &str) -> App {
        let fltk = app::App::default();
        let _ = app::lock(); // enables app::awake() from worker threads
        fltk::window::Window::set_default_xclass(id);
        app::set_visible_focus(true);
        crate::focus::install();
        crate::a11y::install();
        app::set_option(app::Option::ShowScaling, false);
        theme::set(theme::Theme::load());
        // FLTK's own tooltip window stays styled for code that turns it
        // back on; the toolkit draws tooltips itself ([`crate::tooltip`]).
        style_tooltips(&theme::current());
        theme::on_change(style_tooltips);
        crate::tooltip::install();
        // Follow the system's Animations setting (Hyprland, GNOME, macOS, Windows).
        crate::anim::init();
        App { id: id.to_string(), _fltk: fltk, watcher: Some(ThemeWatcher::start()) }
    }

    /// Same as `new` but without live theme following (tests, fixed themes).
    pub fn without_theme_watch(id: &str) -> App {
        let mut a = App::new(id);
        if let Some(w) = a.watcher.take() {
            w.stop();
        }
        a
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// Runs until every window is closed or [`App::quit`] is called.
    /// On return, background tasks are cancelled and their workers joined
    /// (up to 2 s) and theme watching stops.
    pub fn run(mut self) {
        while app::wait() {
            task::drain();
        }
        task::drain();
        self.shutdown();
    }

    /// Processes pending events without blocking (tests and scripted demos).
    pub fn pump() {
        app::check();
        task::drain();
    }

    pub fn quit() {
        app::quit();
    }

    fn shutdown(&mut self) {
        if let Some(w) = self.watcher.take() {
            w.stop();
        }
        let stuck = task::shutdown(Duration::from_secs(2));
        if stuck > 0 {
            eprintln!("fltk_omarchy: {stuck} worker thread(s) did not stop within 2 s");
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn style_tooltips(t: &theme::Theme) {
    // PanelToolTip: tooltip surface, bodySmall text, 10/6 padding, 400 ms delay.
    let bg = t.tooltip.fill(t.palette.background);
    Tooltip::set_color(bg.fl());
    Tooltip::set_text_color(t.tooltip.text.fl());
    Tooltip::set_font(fltk::enums::Font::Helvetica);
    Tooltip::set_font_size(t.font.body_small);
    Tooltip::set_margin_width(t.spacing.control_padding_x);
    Tooltip::set_margin_height(t.spacing.control_padding_y);
    Tooltip::set_delay(0.4);
}

/// Follows `omarchy-theme-set` (it renames `next-theme` over `theme`), edits
/// of `colors.toml`/`shell.toml`, and `~/.config/omarchy/shell.toml`.
/// A watcher thread posts a debounced (150 ms) reload to the UI thread:
/// blocking inotify on Linux (no CPU while idle), a 1 s mtime poll on
/// macOS/Windows that also follows the OS dark/light setting
/// ([`crate::platform`]).
pub struct ThemeWatcher {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ThemeWatcher {
    pub fn start() -> ThemeWatcher {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("omarchy-theme-watch".into())
            .spawn(move || watch_loop(flag))
            .ok();
        ThemeWatcher { stop, thread }
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn watch_loop(stop: Arc<AtomicBool>) {
    crate::platform::watch_theme(stop, &|| {
        task::post(|| {
            theme::reload();
            crate::glass::reload();
            crate::tooltip::reload_settings();
        });
    });
}
