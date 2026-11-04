//! Mouse through the real input path (run by script/os-mouse-check with the
//! user's go-ahead): real clicks on controls, and the native window
//! gestures (header drag = move, bottom-right grip = resize). The app
//! writes the positions of its controls, then a report.
//!   os_mouse_check <geometry.json> <report.json> <seconds>
use fltk::prelude::*;
use fltk_omarchy::layout::{self, FlexExt};
use fltk_omarchy::widgets::*;
use fltk_omarchy::{hyprland, App, AppWindow, Mode, State};
use std::cell::RefCell;
use std::rc::Rc;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (geo_out, out) = (a[1].clone(), a[2].clone());
    let secs: f64 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(20.0);
    let app = App::without_theme_watch("dev.omarchy.fltk.osmouse");
    let mut win = AppWindow::new("os-mouse-check", 480, 360);
    win.set_mode(Mode::Floating);
    win.body();
    let clicks = State::new(0u32);
    let c = clicks.clone();
    let mut r = layout::row();
    let button = Button::new("Click me").bordered().on_click(move || c.update(|n| *n += 1));
    r.fit(&*button);
    let on = State::new(false);
    let sw = ToggleSwitch::new(&on);
    r.fit(&*sw);
    r.close_fitted();
    win.end();
    // Pointer over this window? (the script clicks only when this says 1)
    let hover = format!("{geo_out}.hover");
    let _ = std::fs::write(&hover, "0");
    let mut w0 = win.clone();
    let prev = std::cell::Cell::new(false);
    fltk::app::add_timeout3(0.03, move |h| {
        let (mx, my) = fltk::app::get_mouse();
        let inside = w0.shown() && fltk::app::belowmouse::<fltk::widget::Widget>().is_some()
            && mx >= w0.x_root() && my >= w0.y_root() && mx < w0.x_root() + w0.w() && my < w0.y_root() + w0.h();
        if inside != prev.get() {
            prev.set(inside);
            let _ = std::fs::write(&hover, if inside { "1" } else { "0" });
        }
        let _ = &mut w0;
        fltk::app::repeat_timeout3(0.03, h);
    });
    win.show();
    let first: Rc<RefCell<Option<hyprland::Client>>> = Rc::default();
    let (f2, b2, s2, w2) = (first.clone(), button.clone(), sw.clone(), win.clone());
    fltk::app::add_timeout3(1.0, move |_| {
        let c = hyprland::own_clients(Some("os-mouse-check")).into_iter().next();
        let geo = format!(
            "{{\"button\":[{},{},{},{}],\"switch\":[{},{},{},{}],\"window\":[{},{}],\"client\":{}}}\n",
            b2.x(), b2.y(), b2.w(), b2.h(), s2.x(), s2.y(), s2.w(), s2.h(), w2.w(), w2.h(),
            c.as_ref().map_or("null".into(), |c| format!("{{\"at\":[{},{}],\"size\":[{},{}]}}", c.at.0, c.at.1, c.size.0, c.size.1)));
        *f2.borrow_mut() = c;
        std::fs::write(&geo_out, geo).unwrap();
    });
    let (f3, mut w3) = (first.clone(), win.clone());
    fltk::app::add_timeout3(secs, move |_| {
        let c = hyprland::own_clients(Some("os-mouse-check")).into_iter().next();
        let (moves, resizes) = fltk_omarchy::native::gesture_counts();
        let fmt = |c: &Option<hyprland::Client>| c.as_ref().map_or("null".into(), |c| format!("{{\"at\":[{},{}],\"size\":[{},{}]}}", c.at.0, c.at.1, c.size.0, c.size.1));
        let report = format!("{{\"clicks\":{},\"switch\":{},\"native_moves\":{moves},\"native_resizes\":{resizes},\"before\":{},\"after\":{}}}\n",
            clicks.get(), on.get(), fmt(&f3.borrow()), fmt(&c));
        std::fs::write(&out, report).unwrap();
        w3.hide();
    });
    app.run();
}
