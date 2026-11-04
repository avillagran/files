//! Keyboard through the real input path: script/os-input-check focuses this
//! window, types real keys with `wtype` (Wayland virtual keyboard, delivered
//! by the compositor) and gives focus back. This app records what arrived.
//!   os_input_check <report.json> <seconds>
use fltk::prelude::*;
use fltk_omarchy::focus;
use fltk_omarchy::layout::{self, FlexExt};
use fltk_omarchy::list::{List, ListRow};
use fltk_omarchy::popups::{PopupItem, PopupMenu};
use fltk_omarchy::widgets::*;
use fltk_omarchy::{App, AppWindow, State};
use std::cell::RefCell;
use std::rc::Rc;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let out = a.get(1).cloned().unwrap_or_else(|| "build/os-input.json".into());
    let secs: f64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(12.0);
    let app = App::without_theme_watch("dev.omarchy.fltk.osinput");
    let mut win = AppWindow::new("os-input-check", 520, 420);
    let mut body = win.body();
    let text = State::new(String::new());
    let field = TextField::new().placeholder("type here").bind(&text);
    body.fit(&*field);
    let clicks = State::new(0u32);
    let c = clicks.clone();
    let mut r = layout::row();
    let add = Button::new("Add").on_click(move || c.update(|n| *n += 1));
    r.fit(&*add);
    let group = State::new(0usize);
    let g = ButtonGroup::new(&["Day", "Week", "Month"], &group);
    r.fit(&*g);
    r.close_fitted();
    let tab = State::new(0usize);
    let tabs = TabBar::new(&["Home", "Work", "Music"], &tab);
    body.fit(&*tabs);
    let sel = State::new(None::<usize>);
    let list = List::new(&State::new(50usize), |i| ListRow::new(&format!("Item {i}"))).bind_selected(&sel);
    win.end();
    let picked: Rc<RefCell<Option<usize>>> = Rc::default();
    let (p2, l2) = (picked.clone(), list.clone());
    win.shortcut("F10", move || {
        let p3 = p2.clone();
        PopupMenu::open(&*l2, vec![PopupItem::new("", "Open"), PopupItem::new("", "Copy"), PopupItem::new("", "Paste")], move |i| *p3.borrow_mut() = Some(i));
    });
    win.set_mode(fltk_omarchy::Mode::Floating);
    win.show();
    focus::first(&*win);
    // Focus trace: which control had focus, each time it changed.
    let trace: Rc<RefCell<Vec<String>>> = Rc::default();
    let name = {
        let (f, a, t, l) = (field.clone(), add.clone(), tabs.clone(), list.clone());
        move || -> String {
            if focus::is_focused(&*t) { "tabs".into() } else if focus::is_focused(&*l) { "list".into() }
            else if focus::is_focused(&*a) { "add".into() }
            else if focus::current().is_some_and(|c| c.as_widget_ptr() == f.input().as_widget_ptr()) { "field".into() }
            else if focus::current().is_some() { "other".into() } else { "none".into() }
        }
    };
    let (tr, nm) = (trace.clone(), name.clone());
    fltk::app::add_timeout3(0.02, move |h| {
        let n = nm();
        if tr.borrow().last() != Some(&n) { tr.borrow_mut().push(n); }
        fltk::app::repeat_timeout3(0.02, h);
    });
    let (f2, t2, l3, w2) = (field.clone(), tabs.clone(), list.clone(), win.clone());
    fltk::app::add_timeout3(secs, move |_| {
        let who = if focus::is_focused(&*t2) { "tabs" } else if focus::is_focused(&*l3) { "list" }
            else if focus::current().is_some_and(|c| c.as_widget_ptr() == f2.input().as_widget_ptr()) { "field" } else { "other" };
        let report = format!(
            "{{\"text\":{:?},\"clicks\":{},\"group\":{},\"tab\":{},\"list\":{},\"menu_pick\":{},\"focus_at_end\":\"{who}\",\"focus_trace\":{:?},\"keyboard_mode\":{}}}\n",
            text.get(), clicks.get(), group.get(), tab.get(),
            sel.get().map_or("null".into(), |v| v.to_string()), picked.borrow().map_or("null".into(), |v| v.to_string()),
            trace.borrow(), focus::keyboard_mode());
        std::fs::write(&out, report).unwrap();
        let mut w = w2.clone();
        w.hide();
    });
    app.run();
}
