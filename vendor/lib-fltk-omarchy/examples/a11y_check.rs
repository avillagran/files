//! Real-window accessibility scenario for `script/a11y-check`: a small
//! settings window and a rich text transcript read through AT-SPI (no
//! screen reader, no synthetic input). Prints one line per app-side event
//! so the script can check that an AT action reached the same code as a
//! click. Quits after `secs`.
//!   a11y_check [secs]
use fltk::app;
use fltk_omarchy::a11y::AccessibleExt;
use fltk_omarchy::layout::{self, FlexExt};
use fltk_omarchy::list::{List, ListRow};
use fltk_omarchy::overlay::Dropdown;
use fltk_omarchy::text::RichText;
use fltk_omarchy::widgets::*;
use fltk_omarchy::{App, AppWindow, State};

fn main() {
    let secs: f64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(30.0);
    let app = App::new("dev.omarchy.fltk.a11y");
    let mut win = AppWindow::new("a11y check", 480, 860);
    win.header().action(glyph::SETTINGS, "Preferences", || {
        println!("event preferences")
    });
    let mut body = win.body();
    let presses = State::new(0u32);
    let status = presses.map(|n| format!("Pressed {n} times"));
    body.fit(&*Label::new("").bind(&status));
    let mut row = layout::row();
    let p = presses.clone();
    row.fit(&*Button::new("Press me").bordered().on_click(move || {
        p.update(|n| *n += 1);
        println!("event pressed {}", p.get());
    }));
    row.fit(&*Button::icon(glyph::TRASH, "").accessible_name("Delete"));
    row.close_fitted();
    let name = State::new("Ada".to_string());
    body.fit(&*TextField::new().placeholder("Name").bind(&name));
    let wifi = State::new(true);
    wifi.subscribe(|on| println!("event wifi {on}")).forever();
    body.fit(&*ToggleSwitch::new(&wifi).accessible_name("Wi-Fi"));
    let vol = State::new(0.5);
    vol.subscribe(|v| println!("event volume {v:.2}")).forever();
    body.fit(&*Slider::new(&vol, 0.0, 1.0, 0.1).accessible_name("Volume"));
    let out = State::new(0usize);
    body.fit(&*Dropdown::new("Output", &["Speakers", "Headphones"], &out));
    let count = State::new(10_000usize);
    body.fit(&*List::new(&count, |i| {
        ListRow::new(&format!("Song {i}")).detail("3:30")
    }));
    let rt = RichText::new()
        .on_link(|url| println!("event link {url}"))
        .accessible_name("Transcript");
    rt.set_markdown(
        "# Release notes\n\nThe **parser** streams Markdown. Read [the docs](https://example.com/docs).\n\n- first item\n- second item\n\n```rust\nfn main() {}\n```\n",
    );
    body.fit(&*rt);
    win.end();
    win.show();
    println!("event shown");
    app::add_timeout3(secs, |_| App::quit());
    app.run();
    println!("event quit");
}
