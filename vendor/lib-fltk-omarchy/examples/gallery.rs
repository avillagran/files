//! Component gallery: every classic Omarchy control, live-themed.
//!   cargo run --example gallery
//!   cargo run --example gallery -- --snapshot build/shots/gallery.png
use fltk_omarchy::layout::{self, FlexExt};
use fltk_omarchy::widgets::{glyph, *};
use fltk_omarchy::{App, AppWindow, State};

fn main() {
    let snapshot = std::env::args().skip_while(|a| a != "--snapshot").nth(1);
    let app = App::new("dev.omarchy.fltk.gallery");

    let mut win = AppWindow::new("Omarchy components", 560, 720);
    let mut body = win.body();
    win.header().action(glyph::SETTINGS, "Preferences", || {});
    win.header().destructive_action(glyph::CLOSE, "Close", || fltk::app::quit());

    let hero = Hero::new(glyph::VOLUME_HIGH, "Audio", "Built-in speakers").detail("64%");
    body.fit(&*hero);
    body.fit(&*Separator::new());

    let mut sec = layout::section();
    sec.fit(&*Label::section("Buttons"));
    let mut r = layout::row();
    r.fit(&*Button::new("Default"));
    r.fit(&*Button::new("Bordered").bordered());
    r.fit(&*Button::icon(glyph::PLUS, "Primary").primary());
    r.fit(&*Button::icon(glyph::TRASH, "Delete").danger().bordered());
    layout::spacer();
    r.fit(&*ActionButton::new(glyph::REFRESH, "Refresh"));
    r.close_fitted();
    let period = State::new(1usize);
    let mut r = layout::row();
    r.fit(&*ButtonGroup::new(&["Day", "Week", "Month"], &period));
    layout::spacer();
    r.close_fitted();
    sec.close_fitted();

    let mut sec = layout::section();
    sec.fit(&*Label::section("Inputs"));
    let name = State::new(String::new());
    let mut r = layout::row();
    let field = TextField::new().placeholder("Search apps…").bind(&name);
    r.fit(&*NumberField::new(&State::new(42), 0, 100, 1));
    let _ = field;
    r.close_fitted();
    let volume = State::new(0.64);
    body_slider(&mut sec, &volume);
    sec.close_fitted();

    let mut sec = layout::section();
    sec.fit(&*Label::section("Toggles"));
    sec.fit(&*Toggle::new("Night light", "Warmer colors after sunset", &State::new(true)));
    sec.fit(&*Toggle::new("Do not disturb", "Silence notifications", &State::new(false)));
    sec.close_fitted();

    let mut sec = layout::section();
    sec.fit(&*Label::section("Output"));
    sec.fit(&*Row::new(glyph::MONITOR, "HDMI / DisplayPort").detail("Digital"));
    sec.fit(&*Row::new(glyph::HEADPHONES, "Built-in speakers").detail("Analog").current(true));
    sec.fit(&*Row::new(glyph::BLUETOOTH, "WH-1000XM4").detail("Bluetooth"));
    sec.close_fitted();
    layout::spacer();
    win.end();

    if let Some(path) = snapshot {
        win.snapshot(&path).expect("snapshot");
        println!("snapshot written to {path}");
        return;
    }
    win.show();
    app.run();
}

fn body_slider(sec: &mut fltk::group::Flex, v: &State<f64>) {
    let mut r = layout::row();
    r.fit(&*Label::styled("Volume", TextStyle::Body).tone(Tone::Dim));
    let _s = Slider::new(v, 0.0, 1.0, 0.05);
    let pct = Label::new("64%").right();
    r.fit(&*pct);
    r.close_fitted();
    let _ = sec;
}
