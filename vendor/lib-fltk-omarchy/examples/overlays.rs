//! Overlays: dropdown popup, Omarchy menu, confirm dialog, notification,
//! and a 100 000-row virtual list. With --snapshot DIR it opens each overlay
//! programmatically and writes one PNG per state, without showing a window.
use fltk::prelude::*;
use fltk_omarchy::layout::{self, FlexExt};
use fltk_omarchy::list::{List, ListRow};
use fltk_omarchy::overlay::{ConfirmDialog, Dropdown, Menu, MenuItem, Notification, Urgency};
use fltk_omarchy::widgets::{glyph, *};
use fltk_omarchy::{App, AppWindow, State};

fn main() {
    let shots = std::env::args().skip_while(|a| a != "--snapshot").nth(1);
    let app = App::new("dev.omarchy.fltk.overlays");
    let mut win = AppWindow::new("Overlays", 560, 640);
    let body = win.body();

    let output = State::new(1usize);
    let mut r = layout::row();
    let dd = Dropdown::new("", &["HDMI / DisplayPort", "Built-in speakers", "WH-1000XM4", "USB Audio"], &output);
    r.fit(&*dd);
    let menu_btn = Button::icon(glyph::SEARCH, "Menu").bordered();
    r.fit(&*menu_btn);
    let del_btn = Button::icon(glyph::TRASH, "Delete…").danger().bordered();
    r.fit(&*del_btn);
    r.close_fitted();

    let count = State::new(100_000usize);
    let selected = State::new(Some(2usize));
    let list = List::new(&count, |i| ListRow::new(&format!("Item {i}")).icon(glyph::FILE).detail(&format!("{} KB", i * 3)))
        .bind_selected(&selected);
    let _ = &list;
    body.end();
    win.end();

    let items = vec![
        MenuItem::new("\u{f0047}", "Apps").submenu(),
        MenuItem::new("\u{f0e7}", "Trigger").submenu(),
        MenuItem::new("\u{f03d8}", "Style").submenu(),
        MenuItem::new("\u{f0493}", "Setup").submenu(),
        MenuItem::new("\u{f0954}", "Install").submenu(),
        MenuItem::new("\u{f0425}", "System").submenu(),
    ];
    let w2 = win.clone();
    menu_btn.clone().on_click(move || Menu::open(&*w2, "Go", items.clone(), |i| println!("picked {i}")));
    let w3 = win.clone();
    del_btn.clone().on_click(move || ConfirmDialog::open(&*w3, "Delete 3 files permanently?", "Delete", true, |ok| println!("confirm {ok}")));

    if let Some(dir) = shots {
        let p = |n: &str| format!("{dir}/{n}.png");
        win.snapshot(p("overlays-0-base")).unwrap();
        dd.clone().open();
        win.snapshot(p("overlays-1-dropdown")).unwrap();
        App::pump(); // nothing pending; popup closes on Escape normally
        close_top(&mut win);
        menu_btn.click();
        win.snapshot(p("overlays-2-menu")).unwrap();
        close_top(&mut win);
        del_btn.click();
        win.snapshot(p("overlays-3-confirm")).unwrap();
        close_top(&mut win);
        Notification::show(&*win, glyph::BELL, "Screenshot saved", "~/Pictures/screenshot-2026-10-27.png", Urgency::Normal);
        win.snapshot(p("overlays-4-notification")).unwrap();
        println!("rows built for 100000-row list: {}", list.rows_built());
        return;
    }
    win.show();
    app.run();
}

/// Removes the topmost overlay (last child of the window).
fn close_top(win: &mut AppWindow) {
    let n = win.children();
    if n > 1 {
        if let Some(c) = win.child(n - 1) {
            win.remove(&c);
            fltk::app::delete_widget(c);
        }
    }
}
