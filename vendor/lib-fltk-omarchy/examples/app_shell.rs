//! A file-manager-like shell built only from lib-fltk-omarchy pieces:
//! header with separate Preferences (Ctrl+,) and Actions (F10) menus, tabs,
//! virtual list, searchable dropdown, operation toast with pause/cancel,
//! F1 shortcut help and the interior corner preference.
//!   cargo run --example app_shell
//!   cargo run --example app_shell -- --snapshot build/shots
use fltk::prelude::*;
use fltk_omarchy::layout::{self, FlexExt};
use fltk_omarchy::list::{List, ListRow};
use fltk_omarchy::popups::{OperationToast, PopupItem, PopupMenu, SearchableDropdown, ShortcutHelp};
use fltk_omarchy::task::{Outcome, Task};
use fltk_omarchy::theme::{self, Corners};
use fltk_omarchy::widgets::{glyph, *};
use fltk_omarchy::{App, AppWindow, Mode, State};
use std::rc::Rc;

const BOLT: &str = "\u{f0241}";

fn prefs_items() -> Vec<PopupItem> {
    let c = theme::corners();
    vec![
        PopupItem::new(glyph::WINDOW_FLOAT, "Window: normal").shortcut("").checked(true),
        PopupItem::new("", "Window: fullscreen").shortcut("F11").divider(),
        PopupItem::new("", "Corners: follow Omarchy").checked(c == Corners::Omarchy),
        PopupItem::new("", "Corners: square").checked(c == Corners::Square),
        PopupItem::new("", "Corners: rounded").checked(c == Corners::Rounded),
    ]
}

fn action_items() -> Vec<PopupItem> {
    vec![
        PopupItem::new(glyph::FOLDER, "Open").shortcut("Enter"),
        PopupItem::new(glyph::SEARCH, "Preview").shortcut("Space").divider(),
        PopupItem::new(glyph::FILE, "Copy").shortcut("Ctrl+C"),
        PopupItem::new(glyph::FILE, "Rename").shortcut("F2"),
        PopupItem::new(glyph::TRASH, "Move to trash").shortcut("Delete").disabled(),
    ]
}

fn help() -> Vec<(String, Vec<(String, String)>)> {
    let row = |k: &str, d: &str| (k.to_string(), d.to_string());
    vec![
        ("Move".into(), vec![row("Tab", "Next control"), row("Shift+Tab", "Previous control"), row("Ctrl+Tab", "Next tab"), row("Ctrl+Shift+Tab", "Previous tab")]),
        ("Navigation".into(), vec![row("Ctrl+L", "Edit path"), row("Alt+Left", "Back"), row("Ctrl+T", "New tab")]),
        ("Files".into(), vec![row("F5", "Copy"), row("F6", "Move"), row("Delete", "Move to trash")]),
        ("Window".into(), vec![row("Ctrl+,", "Preferences"), row("F10", "Actions"), row("F1", "This help")]),
    ]
}

fn main() {
    let shots = std::env::args().skip_while(|a| a != "--snapshot").nth(1);
    let app = App::new("dev.omarchy.fltk.shell");
    let mut win = AppWindow::new("Files", 720, 520);
    let mut body = win.body();

    let tab = State::new(0usize);
    let tabs = TabBar::new(&["Home", "Downloads", "Work"], &tab).on_close(|i| println!("close tab {i}"));
    body.fit(&*tabs);

    let place = State::new(None::<usize>);
    let places = vec![
        ("Home".into(), "~/".into()), ("Downloads".into(), "~/Downloads".into()),
        ("Pictures".into(), "~/Pictures".into()), ("Work".into(), "~/Work".into()),
        ("Root".into(), "/".into()), ("Trash".into(), "trash:///".into()),
    ];
    let mut r = layout::row();
    let jump = SearchableDropdown::new("Jump to place…", places, &place);
    r.fit(&*jump);
    layout::spacer();
    let copy_btn = Button::icon(glyph::PLAY, "Copy 3 280 files").bordered();
    r.fit(&*copy_btn);
    r.close_fitted();

    let _list = List::new(&State::new(3280usize), |i| {
        ListRow::new(&format!("photo-{i:04}.jpg")).icon(glyph::FILE).detail(&format!("{:.1} MB", 1.0 + (i % 37) as f64 / 10.0))
    });
    win.end();

    let w = win.clone();
    let gear = win.header().action(glyph::SETTINGS, "Preferences (Ctrl+,)", || {});
    let bolt = win.header().action(BOLT, "Actions (F10)", || {});
    let (g2, b2) = (gear.clone(), bolt.clone());
    let open_prefs = Rc::new(move || {
        let mut w = w.clone();
        PopupMenu::open(&*g2, prefs_items(), move |i| match i {
            0 => w.set_mode(Mode::Normal),
            1 => w.set_mode(Mode::Fullscreen),
            2 => theme::set_corners(Corners::Omarchy),
            3 => theme::set_corners(Corners::Square),
            _ => theme::set_corners(Corners::Rounded),
        });
    });
    let open_actions = Rc::new(move || PopupMenu::open(&*b2, action_items(), |i| println!("action {i}")));
    let (p1, a1) = (open_prefs.clone(), open_actions.clone());
    gear.clone().on_click(move || p1());
    bolt.clone().on_click(move || { a1(); });
    let (p2, a2) = (open_prefs.clone(), open_actions.clone());
    win.shortcut("Ctrl+,", move || p2());
    win.shortcut("F10", move || { a2(); });
    let w3 = win.clone();
    win.shortcut("F1", move || { ShortcutHelp::open(&*w3, "Keyboard shortcuts", help()); });

    let w4 = win.clone();
    let start_copy = move || {
        let Some(toast) = OperationToast::show(&*w4, "Copying 3 280 files") else { return };
        let handle = Task::new(|ctx| { for i in 0..200 { ctx.sleep(std::time::Duration::from_millis(25))?; ctx.progress((i + 1) as f64 / 200.0); } Ok(()) });
        let (mut t1, t2) = (toast.clone(), toast.clone());
        let h = handle.on_progress(move |p| { t1.set_progress(p); t1.set_detail(&format!("{} of 3 280 · to ~/Backup", (p * 3280.0) as u32)); })
            .on_done(move |o: Outcome<()>| { println!("copy {o:?}"); t2.close(); })
            .start();
        let _ = toast.on_cancel(move || h.cancel());
    };
    copy_btn.clone().on_click(start_copy.clone());

    if let Some(dir) = shots {
        let p = |n: &str| format!("{dir}/shell-{n}.png");
        win.snapshot(p("0-base")).unwrap();
        open_prefs();
        win.snapshot(p("1-preferences")).unwrap();
        close_top(&mut win);
        open_actions();
        win.snapshot(p("2-actions")).unwrap();
        close_top(&mut win);
        jump.clone().open();
        win.snapshot(p("3-search")).unwrap();
        close_top(&mut win);
        ShortcutHelp::open(&*win, "Keyboard shortcuts", help());
        win.snapshot(p("4-help")).unwrap();
        close_top(&mut win);
        if let Some(mut t) = OperationToast::show(&*win, "Copying 3 280 files") {
            t.set_progress(0.42);
            t.set_detail("1 378 of 3 280 · to ~/Backup");
            win.snapshot(p("5-operation")).unwrap();
            t.toggle_pause();
            win.snapshot(p("6-paused")).unwrap();
            t.close();
        }
        theme::set_corners(Corners::Rounded);
        win.snapshot(p("7-rounded")).unwrap();
        return;
    }
    win.show();
    app.run();
}

fn close_top(win: &mut AppWindow) {
    let n = win.children();
    if n > 1 {
        if let Some(c) = win.child(n - 1) {
            win.remove(&c);
            fltk::app::delete_widget(c);
        }
    }
}
