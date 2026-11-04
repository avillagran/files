//! Bench workload "minimal": one label and one button, 520x400 (title = last arg).
use fltk_omarchy::layout::FlexExt;
use fltk_omarchy::widgets::{Button, Label};
use fltk_omarchy::{App, AppWindow};

fn main() {
    let title = std::env::args().last().unwrap_or_else(|| "bench-minimal".into());
    let app = App::without_theme_watch("dev.omarchy.fltk.bench");
    let mut win = AppWindow::new(&title, 520, 400);
    let mut body = win.body();
    body.fit(&*Label::new("Hello from lib-fltk-omarchy"));
    body.fit(&*Button::new("Button").bordered());
    win.end();
    win.show();
    app.run();
}
