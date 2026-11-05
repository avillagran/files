//! Component gallery: every classic Omarchy control, live-themed.
//! Preferences (gear, Ctrl+,) switches the language live.
//!   cargo run --example gallery
//!   cargo run --example gallery -- --snapshot build/shots/gallery.png
//!   cargo run --example gallery -- --locale ar --snapshot build/shots/gallery-ar.png
use fltk_omarchy::edit::{CompletionItem, Composer, Trigger};
use fltk_omarchy::layout::{self, FlexExt};
use fltk_omarchy::popups::{PopupItem, PopupMenu};
use fltk_omarchy::text::RichText;
use fltk_omarchy::widgets::{glyph, *};
use fltk_omarchy::{i18n, trn, App, AppWindow, State};
use std::cell::Cell;
use std::rc::Rc;

fn main() {
    let snapshot = std::env::args().skip_while(|a| a != "--snapshot").nth(1);
    let locale = std::env::args().skip_while(|a| a != "--locale").nth(1);
    let app = App::new("dev.omarchy.fltk.gallery");
    if let Some(l) = &locale {
        i18n::set_locale(l);
    }

    let mut win = AppWindow::new("Omarchy components", 560, 940);
    let mut body = win.body();
    // Strings from the toolkit catalog follow the language live; the
    // gallery's own labels have no catalog and stay in English.
    let gear = win.header().action(glyph::SETTINGS, "", || {}).bind_tooltip(&i18n::text("Preferences"));
    win.header().destructive_action(glyph::CLOSE, "", || fltk::app::quit()).bind_tooltip(&i18n::text("Close"));
    let following = Rc::new(Cell::new(locale.is_none()));
    let (g2, f2) = (gear.clone(), following.clone());
    let languages = Rc::new(move || {
        let f3 = f2.clone();
        // The languages in a submenu: Preferences stays short.
        let languages = i18n::LANGUAGES.iter().map(|(code, name)| PopupItem::new("", name).checked(!f2.get() && i18n::language() == *code)).collect();
        let items = vec![
            PopupItem::new("", &i18n::tr("System language")).checked(f2.get()),
            PopupItem::submenu(glyph::SETTINGS, "Language", languages),
        ];
        PopupMenu::open_path(&*g2, items, move |path| {
            f3.set(path[0] == 0);
            match path[..] {
                [1, i] => i18n::set_locale(i18n::LANGUAGES[i].0),
                _ => i18n::follow_system(),
            }
        });
    });
    let l2 = languages.clone();
    gear.clone().on_click(move || l2());
    let l3 = languages.clone();
    win.shortcut("Ctrl+,", move || l3());

    let hero = Hero::new(glyph::VOLUME_HIGH, "Audio", "Built-in speakers").detail("64%");
    body.fit(&*hero);
    body.fit(&*Separator::new());

    let mut sec = layout::section();
    sec.fit(&*Label::section("Buttons"));
    let mut r = layout::row();
    r.fit(&*Button::new("Default"));
    r.fit(&*Button::new("Bordered").bordered());
    r.fit(&*Button::icon(glyph::PLUS, "Primary").primary());
    r.fit(&*Button::icon(glyph::TRASH, "").danger().bordered().bind_text(&i18n::text("Delete")));
    layout::spacer();
    r.fit(&*ActionButton::new(glyph::REFRESH, "").bind_tooltip(&i18n::text("Refresh")));
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
    let field = TextField::new().bind_placeholder(&i18n::text("Search…")).bind(&name);
    let count = State::new(42i64);
    r.fit(&*NumberField::new(&count, 0, 100, 1));
    // Plural forms of the current language (Up/Down in the number field).
    r.fit(&*Label::new("").tone(Tone::Dim).bind(&i18n::map(&count, |n| trn!("{count} items", *n))));
    let _ = field;
    r.close_fitted();
    let volume = State::new(0.64);
    body_slider(&mut sec, &volume);
    sec.close_fitted();

    // Chat input: grows with the text; `$` opens completions at the caret.
    body.fit(&*Label::section("Composer"));
    let draft = State::new(String::new());
    let skills = ["review", "refactor", "explain"];
    body.fit(&*Composer::new().bind(&draft).trigger('$', Trigger::WordStart, move |q, reply| {
        reply.set(skills.iter().filter(|s| s.starts_with(q.text.as_str())).map(|s| CompletionItem::new(s).icon(glyph::PLAY)).collect());
    }).on_submit(|text| println!("send {text}")));

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
    let mut sec = layout::section();
    sec.fit(&*Label::section("").bind(&i18n::text("Language")));
    let mut r = layout::row();
    let name_of = |_: &String| {
        let lang = i18n::language();
        i18n::LANGUAGES.iter().find(|(c, _)| *c == lang).map_or(lang.clone(), |(_, n)| n.to_string())
    };
    r.fit(&*Label::new("").bind(&i18n::map(&i18n::locale(), name_of)));
    layout::spacer();
    let l4 = languages.clone();
    r.fit(&*Button::new("").bordered().bind_text(&i18n::text("Preferences")).on_click(move || l4()));
    r.close_fitted();

    let mut sec = layout::section();
    sec.fit(&*Label::section("Rich text"));
    let rt = RichText::new().padding(0).on_link(|url| println!("open {url}"));
    sec.fit(&*rt);
    sec.fixed(&*rt, 130);
    rt.set_markdown("**Markdown** with `code`, [links](https://omarchy.org) and *streaming*:\n\n- [x] chat transcripts\n- [ ] tables, quotes, code blocks\n\n> Select with the mouse, copy with Ctrl+C.");
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
