//! Window modes against Hyprland, on a hidden workspace (IPC only, no input):
//! floating → normal → floating, plus raw float actions.
//!   mode_check <report>
use fltk::prelude::*;
use fltk_omarchy::{hyprland, App, AppWindow, Mode};
use std::cell::RefCell;
use std::rc::Rc;

fn main() {
    let out = std::env::args().nth(1).unwrap();
    let start_floating = std::env::args().nth(2).as_deref() != Some("normal");
    let app = App::without_theme_watch("dev.omarchy.fltk.modecheck");
    let mut win = AppWindow::new("mode-check", 420, 300);
    win.end();
    if start_floating { win.set_mode(Mode::Floating); }
    win.show();
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let state = |tag: &str| -> String {
        hyprland::own_clients(Some("mode-check")).first()
            .map(|c| format!("\"{tag}\":{{\"floating\":{},\"size\":[{},{}]}}", c.floating, c.size.0, c.size.1))
            .unwrap_or_else(|| format!("\"{tag}\":null"))
    };
    let steps: Vec<(f64, Box<dyn Fn(&mut AppWindow) -> String>)> = vec![
        (0.8, Box::new(move |_| state("start"))),
        (1.0, Box::new(|w| { w.set_mode(Mode::Floating); String::new() })),
        (1.6, Box::new(move |_| state("after_set_mode_floating"))),
        (1.8, Box::new(|w| { w.set_mode(Mode::Normal); String::new() })),
        (2.4, Box::new(move |_| state("after_set_mode_normal"))),
        (2.6, Box::new(|_| { let a = hyprland::own_clients(Some("mode-check"))[0].address.clone();
            hyprland::dispatch(&format!(r#"hl.dsp.window.float({{ window = "address:{a}", action = "unset" }})"#), None); String::new() })),
        (3.2, Box::new(move |_| state("after_raw_unset"))),
        (3.4, Box::new(|_| { let a = hyprland::own_clients(Some("mode-check"))[0].address.clone();
            hyprland::dispatch(&format!(r#"hl.dsp.window.float({{ window = "address:{a}", action = "toggle" }})"#), None); String::new() })),
        (4.0, Box::new(move |_| state("after_raw_toggle"))),
        (4.1, Box::new(|w| { w.set_mode(Mode::Floating); String::new() })),
        (4.5, Box::new(|_| { let a = hyprland::own_clients(Some("mode-check"))[0].address.clone(); hyprland::resize_to(&a, 600, 400); String::new() })),
        (5.0, Box::new(move |_| state("floating_after_ipc_resize_600x400"))),
    ];
    for (t, f) in steps {
        let (l, mut w) = (log.clone(), win.clone());
        fltk::app::add_timeout3(t, move |_| { let s = f(&mut w); if !s.is_empty() { l.borrow_mut().push(s); } });
    }
    let (l, mut w) = (log.clone(), win.clone());
    fltk::app::add_timeout3(5.3, move |_| { std::fs::write(&out, format!("{{{}}}\n", l.borrow().join(","))).unwrap(); w.hide(); });
    app.run();
}
