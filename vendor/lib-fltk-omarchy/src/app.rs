//! Application runtime: fltk init on the UI thread, Omarchy theme applied
//! and watched live (inotify), Omarchy-style tooltips, and the event loop
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
        app::set_option(app::Option::ShowScaling, false);
        theme::set(theme::Theme::load());
        style_tooltips(&theme::current());
        theme::on_change(style_tooltips);
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
/// A blocking inotify thread posts a debounced (150 ms) reload to the UI thread.
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
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
    if fd < 0 {
        return;
    }
    let mask = libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO | libc::IN_CREATE | libc::IN_DELETE | libc::IN_DELETE_SELF | libc::IN_MOVE_SELF;
    let add = |p: &std::path::Path| -> i32 {
        let c = CString::new(p.as_os_str().as_bytes()).unwrap_or_default();
        unsafe { libc::inotify_add_watch(fd, c.as_ptr(), mask) }
    };
    let state = theme::omarchy_state_dir();
    let user = theme::user_shell_toml();
    add(&state);
    let mut theme_wd = add(&theme::theme_dir());
    if let Some(dir) = user.parent() {
        add(dir);
    }
    // Omarchy's window transparency toggles (Glassy windows). The directory
    // only exists once a toggle was set, so its parents are watched too.
    let toggles = crate::glass::toggles_dir();
    let mut toggles_wd = add(&toggles);
    let toggle_parents: Vec<i32> = toggles.ancestors().skip(1).take(2).map(add).collect();
    let mut buf = [0u8; 4096];
    let mut pending: Option<std::time::Instant> = None;
    while !stop.load(Ordering::Relaxed) {
        let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
        let wait_ms = if pending.is_some() { 30 } else { 250 };
        let ready = unsafe { libc::poll(&mut pfd, 1, wait_ms) };
        if ready > 0 {
            loop {
                let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
                if n <= 0 {
                    break;
                }
                let mut off = 0usize;
                while off < n as usize {
                    let ev = unsafe { &*(buf.as_ptr().add(off) as *const libc::inotify_event) };
                    let name_bytes = &buf[off + std::mem::size_of::<libc::inotify_event>()..off + std::mem::size_of::<libc::inotify_event>() + ev.len as usize];
                    let name = String::from_utf8_lossy(name_bytes).trim_end_matches('\0').to_string();
                    let relevant = matches!(name.as_str(), "theme" | "theme.name" | "colors.toml" | "shell.toml")
                        || ev.wd == theme_wd
                        || ev.wd == toggles_wd
                        || (toggle_parents.contains(&ev.wd) && matches!(name.as_str(), "toggles" | "hypr"));
                    if relevant {
                        pending = Some(std::time::Instant::now());
                    }
                    off += std::mem::size_of::<libc::inotify_event>() + ev.len as usize;
                }
            }
        }
        if let Some(t0) = pending {
            if t0.elapsed() >= Duration::from_millis(150) {
                pending = None;
                // The theme directory may have been replaced: watch the new one.
                unsafe { libc::inotify_rm_watch(fd, theme_wd) };
                theme_wd = add(&theme::theme_dir());
                if toggles_wd < 0 {
                    toggles_wd = add(&toggles);
                }
                task::post(|| {
                    theme::reload();
                    crate::glass::reload();
                });
            }
        }
    }
    unsafe { libc::close(fd) };
}
