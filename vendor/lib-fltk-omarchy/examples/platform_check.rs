//! Platform layer check in a real window, on any OS: prints what the
//! platform module detected (OS, Wayland, Hyprland, appearance, font,
//! directories, primary modifier) and shows a floating `AppWindow` for a
//! few seconds, logging its geometry so a driver can drag its header and
//! bottom-right corner and see the window follow (the FLTK fallback used on
//! X11 window managers, macOS and Windows). Writes a JSON report.
//!   platform_check <report.json> [seconds]
//! `script/x11-check` runs it in an isolated Xvfb and drags it with XTest.
use fltk::{app, prelude::*};
use fltk_omarchy::platform::{self, dirs};
use fltk_omarchy::{hyprland, keys, native, theme, widgets::Label, App, AppWindow, Mode};
use std::time::{Duration, Instant};

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "platform-check.json".into());
    let secs: f64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(6.0);
    let app = App::new("dev.omarchy.fltk.platformcheck");
    let mut win = AppWindow::new("platform-check", 420, 300);
    let mut body = win.body();
    let label = Label::new(&format!("{} · Preferences: {}", platform::OS.name(), keys::label("Mod+,")));
    fltk_omarchy::layout::FlexExt::fit(&mut body, &*label);
    win.end();
    win.set_mode(Mode::Floating);
    win.set_pos(100, 100);
    win.show();
    let t0 = Instant::now();
    let mut log: Vec<String> = Vec::new();
    let mut last = (0, 0, 0, 0);
    while t0.elapsed() < Duration::from_secs_f64(secs) && win.shown() {
        let _ = app::wait_for(0.02);
        let g = (win.x(), win.y(), win.w(), win.h());
        if g != last {
            log.push(format!("{{\"t\":{:.2},\"x\":{},\"y\":{},\"w\":{},\"h\":{}}}", t0.elapsed().as_secs_f64(), g.0, g.1, g.2, g.3));
            last = g;
        }
    }
    let t = theme::current();
    let q = |s: String| format!("{s:?}");
    let report = format!(
        "{{\"os\":{},\"wayland\":{},\"hyprland\":{},\"prefers_dark\":{},\"theme\":{},\"accent\":{},\"font\":{},\"primary\":{},\"config\":{},\"state\":{},\"cpu_s\":{},\"native_gestures\":[{},{}],\"geometry\":[{}]}}",
        q(platform::OS.name().into()),
        platform::wayland(),
        hyprland::available(),
        platform::prefers_dark().map_or("null".into(), |d| d.to_string()),
        q(t.name.clone()),
        q(t.palette.accent.hex()),
        q(t.font_family.clone()),
        q(keys::label("Mod+,")),
        q(dirs::config().display().to_string()),
        q(dirs::state().display().to_string()),
        platform::process_cpu_time().map_or(-1.0, |d| d.as_secs_f64()),
        native::gesture_counts().0,
        native::gesture_counts().1,
        log.join(",")
    );
    std::fs::write(&out, &report).expect("write report");
    println!("{report}");
    drop(app);
}
