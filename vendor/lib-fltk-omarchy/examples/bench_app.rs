//! Bench workload "app": header, bound input/label, buttons, progress and a
//! 100 000-row virtualized list, 520x400 (title = last arg). Live theme on.
use fltk_omarchy::layout::{self, FlexExt};
use fltk_omarchy::list::{List, ListRow};
use fltk_omarchy::widgets::*;
use fltk_omarchy::{combine, App, AppWindow, State};

fn main() {
    let title = std::env::args().last().unwrap_or_else(|| "bench-app".into());
    let app = App::new("dev.omarchy.fltk.bench");
    let mut win = AppWindow::new(&title, 520, 400);
    let mut body = win.body();
    let name = State::new("Omarchy".to_string());
    let clicks = State::new(0u32);
    body.fit(&*TextField::new().bind(&name));
    let greeting = combine(&name, &clicks, |n, c| format!("Hello {n} ({c})"));
    body.fit(&*Label::new("").bold().bind(&greeting));
    let mut r = layout::row();
    let c1 = clicks.clone();
    r.fit(&*Button::new("Add").primary().on_click(move || c1.update(|c| *c += 1)));
    let c2 = clicks.clone();
    r.fit(&*Button::new("Reset").bordered().on_click(move || c2.set(0)));
    r.close_fitted();
    body.fit(&*Progress::new(&State::new(0.4)));
    let _list = List::new(&State::new(100_000usize), |i| ListRow::new(&format!("Item {i}")).detail(&format!("{} KB", i * 3)));
    win.end();
    win.show();
    app.run();
}
