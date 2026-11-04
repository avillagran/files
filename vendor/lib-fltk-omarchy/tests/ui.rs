//! UI tests on the main thread (fltk is single-threaded), offscreen: no
//! window is shown. Each check renders with `snapshot` and reads pixels.
//! Uses a fake Omarchy state dir so results do not depend on the desktop.
use fltk::prelude::*;
use fltk_omarchy::focus::{self, press};
use fltk_omarchy::gfx::{self, Hit, View};
use fltk_omarchy::layout::{self, FlexExt};
use fltk_omarchy::list::{List, ListRow};
use fltk_omarchy::panel::{KeyboardPanel, PanelLayout};
use fltk_omarchy::overlay::{ConfirmDialog, Dropdown, Menu, MenuItem, Notification, Urgency};
use fltk_omarchy::popups::{Dismiss, OperationToast, PopupCard, PopupItem, PopupMenu, SearchableDropdown, ShortcutHelp};
use fltk_omarchy::table::{Column, Table, Tree, TreeNode};
use fltk_omarchy::theme::Corners;
use fltk_omarchy::Mode;
use fltk_omarchy::task::{Outcome, Task};
use fltk_omarchy::widgets::*;
use fltk_omarchy::{theme, App, AppWindow, State};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

const COLORS_A: &str = "accent = \"#e68e0d\"\nbackground = \"#121212\"\nforeground = \"#bebebe\"\nred = \"#d35f5f\"\n";
const COLORS_B: &str = "accent = \"#33ccff\"\nbackground = \"#1a1b26\"\nforeground = \"#c0caf5\"\nred = \"#f7768e\"\n";

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("build/test-ui");
    let _ = std::fs::remove_dir_all(&root);
    let state = root.join("state");
    std::fs::create_dir_all(state.join("theme")).unwrap();
    std::fs::write(state.join("theme/colors.toml"), COLORS_A).unwrap();
    std::fs::write(state.join("theme.name"), "A").unwrap();
    std::env::set_var("OMARCHY_FLTK_STATE_DIR", &state);
    std::env::set_var("OMARCHY_FLTK_USER_SHELL", root.join("no-user-shell.toml"));
    // Hyprland values are read live; pin them by hiding the socket.
    std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");

    let app = App::new("dev.omarchy.fltk.tests");
    let shots = root.join("shots");
    let mut failed = 0;
    let mut run = |name: &str, f: &dyn Fn(&Path) -> Result<(), String>| {
        let r = f(&shots);
        match &r {
            Ok(()) => println!("ok   {name}"),
            Err(e) => { failed += 1; println!("FAIL {name}: {e}") }
        }
    };
    run("theme loads omarchy colors and shell tokens", &|_| {
        let t = theme::current();
        check(t.palette.accent == theme::Rgb(0xe68e0d), "accent")?;
        check(t.spacing.control_height == 28 && t.font.body == 12, "tokens")
    });
    run("primary button is painted with the accent", &|shots| {
        let mut win = AppWindow::new("t", 300, 140);
        let mut body = win.body();
        let b = Button::new("Go").primary();
        body.fit(&*b);
        win.end();
        let p = shots.join("button.png");
        win.snapshot(&p).map_err(|e| e.to_string())?;
        let (x, y) = (b.x() + 3, b.y() + 3);
        let px = pixel(&p, x, y)?;
        check(px == [0xe6, 0x8e, 0x0d], &format!("pixel {px:?} at {x},{y}"))
    });
    run("text field two-way binding and placeholder", &|shots| {
        let name = State::new(String::new());
        let mut win = AppWindow::new("t", 300, 140);
        let mut body = win.body();
        let mut f = TextField::new().placeholder("Name").bind(&name);
        body.fit(&*f);
        win.end();
        win.snapshot(shots.join("field-empty.png")).map_err(|e| e.to_string())?;
        f.type_text("Omarchy");
        check(name.get() == "Omarchy", "typed text reaches the State")?;
        name.set("FLTK".into());
        check(f.value() == "FLTK", "State reaches the field")
    });
    run("toggle, switch, slider, button group follow their State", &|_| {
        let on = State::new(false);
        let t = Toggle::new("Night light", "", &on);
        t.toggle();
        check(on.get() && t.is_on(), "toggle")?;
        let v = State::new(0.2);
        let s = Slider::new(&v, 0.0, 1.0, 0.1);
        v.set(0.7);
        check((s.value() - 0.7).abs() < 1e-9, "slider")?;
        let sel = State::new(0usize);
        let g = ButtonGroup::new(&["A", "B", "C"], &sel);
        g.button(2).unwrap().click();
        check(sel.get() == 2 && g.selected() == 2, "group")
    });
    run("button click, bound text and enabled state", &|_| {
        let clicks = Rc::new(RefCell::new(0));
        let c = clicks.clone();
        let label = State::new("Save".to_string());
        let enabled = State::new(true);
        let b = Button::new("x").bind_text(&label).bind_enabled(&enabled).on_click(move || *c.borrow_mut() += 1);
        b.click();
        label.set("Saved".into());
        enabled.set(false);
        check(*clicks.borrow() == 1 && b.text() == "Saved" && !b.active(), "button state")
    });
    run("virtual list builds only visible rows of 1 000 000", &|shots| {
        let count = State::new(1_000_000usize);
        let mut win = AppWindow::new("t", 400, 400);
        let mut list = List::new(&count, |i| ListRow::new(&format!("Row {i}")));
        win.end();
        win.snapshot(shots.join("list.png")).map_err(|e| e.to_string())?;
        let first = list.rows_built();
        list.select(500_000);
        win.snapshot(shots.join("list-mid.png")).map_err(|e| e.to_string())?;
        check(first <= 16 && list.rows_built() - first <= 16, &format!("built {first} then {}", list.rows_built() - first))?;
        check(list.selected() == Some(500_000), "selection")
    });
    run("menu overlay opens on top and filter ranks", &|shots| {
        let mut win = AppWindow::new("t", 400, 400);
        win.end();
        let before = win.children();
        Menu::open(&*win, "Go", vec![MenuItem::new("", "Apps"), MenuItem::new("", "System")], |_| {});
        win.snapshot(shots.join("menu.png")).map_err(|e| e.to_string())?;
        check(win.children() == before + 1, "layer added")
    });
    run("notification toast is stacked and counted", &|shots| {
        let mut win = AppWindow::new("t", 420, 300);
        win.end();
        Notification::show(&*win, "", "Saved", "body", Urgency::Critical);
        Notification::show(&*win, "", "Second", "", Urgency::Critical);
        win.snapshot(shots.join("toasts.png")).map_err(|e| e.to_string())?;
        check(Notification::count() == 2, "two toasts")
    });
    run("task progress/done run on the UI thread; cancel stops work", &|_| {
        let main = std::thread::current().id();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let (s1, s2) = (seen.clone(), seen.clone());
        let h = Task::new(|ctx| { for i in 0..5 { ctx.progress(i as f64); std::thread::sleep(Duration::from_millis(5)); } Ok(42) })
            .on_progress(move |p| s1.borrow_mut().push((std::thread::current().id() == main, p)))
            .on_done(move |o| s2.borrow_mut().push((std::thread::current().id() == main, if o == Outcome::Done(42) { 42.0 } else { -1.0 })))
            .start();
        pump_until(|| h.is_finished() && seen.borrow().last().is_some_and(|x| x.1 == 42.0));
        check(seen.borrow().iter().all(|x| x.0), "callbacks on main thread")?;
        let done = Rc::new(RefCell::new(None));
        let d2 = done.clone();
        let h = Task::new(|ctx| { loop { ctx.sleep(Duration::from_millis(10))?; } })
            .on_done(move |o: Outcome<()>| *d2.borrow_mut() = Some(o)).start();
        std::thread::sleep(Duration::from_millis(30));
        let t0 = Instant::now();
        h.cancel();
        pump_until(|| done.borrow().is_some());
        check(*done.borrow() == Some(Outcome::Cancelled) && t0.elapsed() < Duration::from_millis(500), "cancelled quickly")
    });
    run("corner preference: square vs rounded button corner pixel", &|shots| {
        let mut win = AppWindow::new("t", 300, 140);
        let mut body = win.body();
        let b = Button::new("Go").primary();
        body.fit(&*b);
        win.end();
        theme::set_corners(Corners::Square);
        let p = shots.join("corner-square.png");
        win.snapshot(&p).map_err(|e| e.to_string())?;
        let square = pixel(&p, b.x(), b.y())?;
        theme::set_corners(Corners::Rounded);
        let p = shots.join("corner-rounded.png");
        win.snapshot(&p).map_err(|e| e.to_string())?;
        let rounded = pixel(&p, b.x(), b.y())?;
        theme::set_corners(Corners::Omarchy);
        let bg = theme::current().palette.background;
        check(square != [bg.r(), bg.g(), bg.b()], &format!("square corner painted {square:?}"))?;
        check(rounded == [bg.r(), bg.g(), bg.b()], &format!("rounded corner shows background {rounded:?}"))?;
        check(theme::current().badge_radius() == 8, "badges keep a fixed radius")
    });
    run("popup menu, tab bar, searchable filter, window mode parse", &|shots| {
        let mut win = AppWindow::new("t", 400, 300);
        let mut body = win.body();
        let tab = State::new(0usize);
        let closed = Rc::new(RefCell::new(None));
        let c2 = closed.clone();
        let tabs = TabBar::new(&["A", "B"], &tab).on_close(move |i| *c2.borrow_mut() = Some(i));
        body.fit(&*tabs);
        let anchor = Button::new("Menu");
        body.fit(&*anchor);
        win.end();
        let before = win.children();
        PopupMenu::open(&*anchor, vec![PopupItem::new("", "One").shortcut("Ctrl+1"), PopupItem::new("", "Two").disabled()], |_| {});
        win.snapshot(shots.join("popup.png")).map_err(|e| e.to_string())?;
        check(win.children() == before + 1, "popup layer added")?;
        tabs.request_close(1);
        check(*closed.borrow() == Some(1), "tab close callback")?;
        let opts = vec![("Home".to_string(), "~/".to_string()), ("Work".to_string(), "~/Work".to_string())];
        check(SearchableDropdown::filter(&opts, "work") == vec![1], "filter by label")?;
        check(SearchableDropdown::filter(&opts, "~/") == vec![0, 1], "filter by description")?;
        check(Mode::parse("fullscreen") == Some(Mode::Fullscreen) && Mode::Normal.as_str() == "normal", "mode parse")
    });
    run("operation toast pause/cancel callbacks", &|_| {
        let mut win = AppWindow::new("t", 420, 300);
        win.end();
        let paused = Rc::new(RefCell::new(Vec::new()));
        let cancelled = Rc::new(RefCell::new(false));
        let (p2, c2) = (paused.clone(), cancelled.clone());
        let mut t = OperationToast::show(&*win, "Copying").ok_or("toast")?
            .on_pause(move |p| p2.borrow_mut().push(p)).on_cancel(move || *c2.borrow_mut() = true);
        t.set_progress(0.5);
        t.toggle_pause();
        t.toggle_pause();
        t.cancel();
        t.close();
        check(*paused.borrow() == vec![true, false] && *cancelled.borrow(), "callbacks")
    });
    run("bindings live as long as the widget, not the Rust handle", &|_| {
        let sel = State::new(0usize);
        let mut win = AppWindow::new("t", 300, 140);
        let mut r = layout::row();
        r.fit(&*ButtonGroup::new(&["A", "B"], &sel)); // handle dropped here
        r.fit(&*Label::new("").bind(&sel.map(|v| v.to_string())));
        r.close_fitted();
        win.end();
        check(sel.listener_count() == 2, &format!("group binding (plus the map) kept after the handles were dropped: {}", sel.listener_count()))?;
        let before = fltk_omarchy::state::widget_binding_count();
        let w = (*win).clone();
        fltk::window::Window::delete(w);
        App::pump();
        let _probe = Label::new("").bind(&State::new(String::new())); // prunes dead bindings
        check(fltk_omarchy::state::widget_binding_count() < before, "bindings of deleted widgets are released")?;
        check(sel.listener_count() == 1, &format!("only the map's own listener is left: {}", sel.listener_count()))
    });
    run("opening a layer while a dropdown is open does not crash", &|_| {
        let mut win = AppWindow::new("t", 400, 300);
        let mut body = win.body();
        let v = State::new(0usize);
        let mut dd = Dropdown::new("Output", &["A", "B"], &v);
        body.fit(&*dd);
        win.end();
        let before = win.children();
        dd.open();
        ConfirmDialog::open(&*win, "Sure?", "Yes", false, |_| {});
        pump_until(|| win.children() == before + 1);
        check(win.children() == before + 1, "dropdown closed, dialog open")?;
        press("Escape");
        check(win.children() == before, "dialog closed")
    });
    run("popup menu: 12 entries in 800x440 at a large font clamp, scroll and stay usable", &|shots| {
        let user = PathBuf::from(std::env::var("OMARCHY_FLTK_USER_SHELL").unwrap());
        std::fs::write(&user, "[font]\nbase-size = 18\n").unwrap();
        theme::reload();
        let mut win = AppWindow::new("t", 800, 440);
        let _body = win.body();
        let mut r = layout::row();
        layout::spacer();
        let anchor = Button::new("Actions");
        r.fit(&*anchor);
        r.close_fitted();
        win.end();
        win.snapshot(shots.join("popup-0.png")).map_err(|e| e.to_string())?; // lays out
        let mut a2 = anchor.clone();
        let _ = a2.take_focus();
        let mut items: Vec<PopupItem> = (0..12).map(|i| PopupItem::new("", &format!("Action {i}")).shortcut(&format!("Ctrl+{i}"))).collect();
        items[0] = PopupItem::new("", "A label far too long to fit in this small window, with words and words and more words to cut").shortcut("Ctrl+L");
        items[3] = items[3].clone().divider();
        items[5] = items[5].clone().disabled();
        items[11] = items[11].clone().checked(true);
        let picked = Rc::new(RefCell::new(Vec::new()));
        let p2 = picked.clone();
        let reasons = Rc::new(RefCell::new(Vec::new()));
        let r2 = reasons.clone();
        let h = PopupMenu::open(&*anchor, items.clone(), move |i| p2.borrow_mut().push(i)).on_dismiss(move |d| r2.borrow_mut().push(d));
        let (x, y, w, hh) = h.rect().ok_or("rect")?;
        let (_, content) = PopupMenu::measure(&items, 440, anchor.y() + anchor.h());
        check(y >= 0 && y + hh <= 440 && x >= 0 && x + w <= 800, &format!("card inside the window: {:?}", h.rect()))?;
        check(content > hh, &format!("entries ({content}px) taller than the card ({hh}px): it scrolls"))?;
        check(x + w <= anchor.x() + anchor.w() + 1 || x <= 2, &format!("right-aligned under a right-side anchor, or clamped: {:?} anchor {}..{}", h.rect(), anchor.x(), anchor.x() + anchor.w()))?;
        win.snapshot(shots.join("popup-1-top.png")).map_err(|e| e.to_string())?;
        press("End");
        win.snapshot(shots.join("popup-2-end.png")).map_err(|e| e.to_string())?;
        // The last row is now at the bottom of the card: click it.
        let row = theme::current().spacing.popup_row_height;
        focus::click_at(&*win, x + w / 2, y + hh - row / 2 - 3);
        check(*picked.borrow() == vec![11], &format!("click on the scrolled last row picks it: {:?}", picked.borrow()))?;
        check(*reasons.borrow() == vec![Dismiss::Picked(11)] && !h.is_open(), "dismissed with Picked(11)")?;
        check(focus::is_focused(&*anchor), "focus back on the anchor")?;
        // Keys: Home, PageDown skips past the disabled entry, Up wraps.
        picked.borrow_mut().clear();
        let p3 = picked.clone();
        PopupMenu::open(&*anchor, items.clone(), move |i| p3.borrow_mut().push(i));
        press("Down"); press("Down"); press("Down"); press("Down"); press("Down"); // 0→1→2→3→4→6
        press("Enter");
        check(*picked.borrow() == vec![6], &format!("Down skips the disabled entry: {:?}", picked.borrow()))?;
        let p4 = picked.clone();
        PopupMenu::open(&*anchor, items.clone(), move |i| p4.borrow_mut().push(i));
        press("Up"); press("Enter");
        check(picked.borrow().last() == Some(&11), "Up from the first entry wraps to the last")?;
        let p5 = picked.clone();
        PopupMenu::open(&*anchor, items.clone(), move |i| p5.borrow_mut().push(i));
        press("PageDown"); press("PageDown"); press("Home"); press("Enter");
        check(picked.borrow().last() == Some(&0), "Home goes back to the first entry")?;
        std::fs::remove_file(&user).unwrap();
        theme::reload();
        Ok(())
    });
    run("popup menu fits 800x440 at font sizes 12, 18 and 24", &|_| {
        let user = PathBuf::from(std::env::var("OMARCHY_FLTK_USER_SHELL").unwrap());
        for size in [12, 18, 24] {
            std::fs::write(&user, format!("[font]\nbase-size = {size}\n")).unwrap();
            theme::reload();
            let mut win = AppWindow::new("t", 800, 440);
            let mut body = win.body();
            let anchor = Button::new("Actions");
            body.fit(&*anchor);
            win.end();
            win.snapshot(std::env::temp_dir().join("omarchy-fltk-unused.png")).ok();
            let items: Vec<PopupItem> = (0..12).map(|i| PopupItem::new("", &format!("Action {i}")).shortcut("Ctrl+Shift+F12")).collect();
            let got = Rc::new(RefCell::new(None));
            let g2 = got.clone();
            let h = PopupMenu::open(&*anchor, items, move |i| *g2.borrow_mut() = Some(i));
            let (x, y, w, hh) = h.rect().ok_or("rect")?;
            check(x >= 0 && y >= 0 && x + w <= 800 && y + hh <= 440, &format!("size {size}: card {:?} inside 800x440", h.rect()))?;
            press("End"); press("Enter");
            check(*got.borrow() == Some(11), &format!("size {size}: last entry reachable"))?;
            fltk::window::Window::delete((*win).clone());
        }
        std::fs::remove_file(&user).unwrap();
        theme::reload();
        check(theme::current().font.base == 12, "font restored")
    });
    run("popup menu lifecycle: Esc, click outside, close(), deleted window", &|_| {
        let mut win = AppWindow::new("t", 400, 300);
        let mut body = win.body();
        let anchor = Button::new("Menu");
        body.fit(&*anchor);
        win.end();
        win.snapshot(std::env::temp_dir().join("omarchy-fltk-unused.png")).ok();
        let f10 = Rc::new(RefCell::new(0));
        let f2 = f10.clone();
        win.shortcut("F10", move || *f2.borrow_mut() += 1);
        let items = || vec![PopupItem::new("", "One"), PopupItem::new("", "Two")];
        let picks = Rc::new(RefCell::new(0));
        let mut a = anchor.clone();
        let _ = a.take_focus();
        let pk = picks.clone();
        let h = PopupMenu::open(&*anchor, items(), move |_| *pk.borrow_mut() += 1);
        press("Escape");
        check(h.dismissed() == Some(Dismiss::Escape) && focus::is_focused(&*anchor), "Esc: reason and focus")?;
        press("F10");
        check(*f10.borrow() == 1, "window shortcut works right after Esc")?;
        let pk = picks.clone();
        let h = PopupMenu::open(&*anchor, items(), move |_| *pk.borrow_mut() += 1);
        focus::click_at(&*win, 390, 290);
        check(h.dismissed() == Some(Dismiss::ClickOutside) && *picks.borrow() == 0, "click outside: no pick")?;
        let pk = picks.clone();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let s2 = seen.clone();
        let h = PopupMenu::open(&*anchor, items(), move |_| *pk.borrow_mut() += 1).on_dismiss(move |d| s2.borrow_mut().push(d));
        check(h.is_open(), "open")?;
        h.close();
        h.close();
        let s3 = seen.clone();
        let _ = h.clone().on_dismiss(move |d| s3.borrow_mut().push(d));
        check(*seen.borrow() == vec![Dismiss::Closed, Dismiss::Closed] && !h.is_open() && *picks.borrow() == 0,
              &format!("close(): once, no pick, late on_dismiss runs now: {:?}", seen.borrow()))?;
        check(focus::is_focused(&*anchor), "focus back after close()")?;
        // Window deleted while the menu is open: no callback, no panic.
        let fired = Rc::new(RefCell::new(false));
        let f3 = fired.clone();
        let h = PopupMenu::open(&*anchor, items(), |_| {}).on_dismiss(move |_| *f3.borrow_mut() = true);
        fltk::window::Window::delete((*win).clone());
        App::pump();
        h.close();
        check(!h.is_open() && !*fired.borrow(), "deleted window: not open, no callback")
    });
    run("keyboard panels: dual pane in one canvas, path field, app Ctrl+Tab once, menus", &|shots| {
        let mut win = AppWindow::new("t", 640, 360);
        let mut body = win.body();
        let path = State::new(String::new());
        let field = TextField::new().placeholder("Path").bind(&path);
        body.fit(&*field);
        let left = KeyboardPanel::new("left");
        let right = KeyboardPanel::new("right").layout(PanelLayout::Grid { columns: 4 });
        // Stable IDs, not indexes.
        left.set_items(20, |i| 1000 + i as u64, |id| id.checked_sub(1000).map(|x| x as usize));
        right.set_items(12, |i| 5000 + 3 * i as u64, |id| id.checked_sub(5000).filter(|x| x % 3 == 0).map(|x| (x / 3) as usize));
        let mut canvas = fltk::widget::Widget::default().with_size(600, 220);
        canvas.handle(|_, ev| matches!(ev, fltk::enums::Event::Focus | fltk::enums::Event::Unfocus));
        let (l2, r2) = (left.clone(), right.clone());
        canvas.draw(move |w| theme::with(|t| {
            let half = w.w() / 2;
            for (i, p) in [&l2, &r2].into_iter().enumerate() {
                let x = w.x() + i as i32 * half;
                if let Some(c) = p.cursor_index() {
                    fltk_omarchy::paint::fill(x + 8, w.y() + 8 + 20 * (c as i32 % 8), half - 16, 18, 0, t.fill(&t.controls.selected, t.palette.background));
                }
                if p.show_ring() {
                    fltk_omarchy::paint::stroke(x + 2, w.y() + 2, half - 4, w.h() - 4, t.radius, 2, t.border(&t.controls.focus, t.palette.background));
                }
            }
        }));
        body.fit(&canvas);
        focus::host(&canvas, &[&left, &right]);
        let after = Button::new("After");
        body.fit(&*after);
        let activated = Rc::new(RefCell::new(Vec::new()));
        let a2 = activated.clone();
        right.on_activate(move |id| a2.borrow_mut().push(id));
        let r3 = right.clone();
        left.on_edge(move |k| { if k == fltk::enums::Key::Right { r3.focus(); true } else { false } });
        let tabs_switched = Rc::new(RefCell::new(0));
        let t2 = tabs_switched.clone();
        win.shortcut("Ctrl+Tab", move || *t2.borrow_mut() += 1); // the app's own tabs
        win.end();
        win.snapshot(shots.join("panels-0.png")).map_err(|e| e.to_string())?;

        focus::first(&*win).ok_or("first")?;
        press("Shift+w"); press("o");
        check(path.get() == "Wo", &format!("path edit: {:?}", path.get()))?;
        press("Tab");
        check(left.has_focus() && !right.has_focus() && focus::focused_panel() == Some(left.clone()), "Tab enters the left panel")?;
        press("Down"); press("Down");
        check(left.cursor() == Some(1001) && left.selection() == vec![1001], "cursor and selection by ID")?;
        press("Tab");
        check(right.has_focus() && focus::is_focused(&canvas), "Tab: second stop in the same widget")?;
        press("Right"); press("Right"); press("Down");
        check(right.cursor_index() == Some(5) && right.cursor() == Some(5015), &format!("grid moves: {:?}", right.cursor_index()))?;
        press("Shift+Left");
        check(right.selection() == vec![5012, 5015], &format!("Shift range: {:?}", right.selection()))?;
        win.snapshot(shots.join("panels-1-right.png")).map_err(|e| e.to_string())?;
        let ring = pixel(&shots.join("panels-1-right.png"), canvas.x() + canvas.w() / 2 + 3, canvas.y() + 40)?;
        let off = pixel(&shots.join("panels-1-right.png"), canvas.x() + 3, canvas.y() + 40)?;
        let acc = theme::current().palette.accent;
        check(ring == [acc.r(), acc.g(), acc.b()] && off != ring, &format!("ring on the right panel only: {ring:?} / {off:?}"))?;
        press("Enter");
        check(*activated.borrow() == vec![5012], &format!("Enter activates the cursor item: {:?}", activated.borrow()))?;
        press("Ctrl+Tab");
        check(*tabs_switched.borrow() == 1 && right.has_focus() && right.cursor() == Some(5012), "the app's Ctrl+Tab runs once; panels unchanged")?;
        press("Shift+Tab");
        check(left.has_focus(), "Shift+Tab back to the left panel")?;
        press("Right");
        check(right.has_focus(), "Right at the list edge goes to the other panel (on_edge)")?;
        // A menu opened from the panel gives focus back to the same panel.
        let h = PopupMenu::open(&canvas, vec![PopupItem::new("", "Copy"), PopupItem::new("", "Paste")], |_| {});
        check(!right.has_focus(), "menu has the keyboard")?;
        press("Escape");
        check(h.dismissed() == Some(Dismiss::Escape) && right.has_focus(), "Esc: back to the right panel")?;
        press("Tab");
        check(focus::is_focused(&*after), "Tab leaves the canvas after its last panel")?;
        press("Tab");
        check(focus::current().is_some_and(|f| f.as_widget_ptr() == field.input().as_widget_ptr()), "wraps to the path field")?;
        // F9-like: one panel only; the other loses its host.
        focus::host(&canvas, &[&left]);
        right.focus();
        check(!right.has_focus(), "a removed panel cannot take focus")?;
        left.focus();
        check(left.has_focus() && focus::is_focused(&canvas), "panel.focus() from code")
    });
    run("gfx View: paint with alpha, area limit, budget, hit shape, keyboard, pause, theme", &|shots| {
        let acc = theme::current().palette.accent;
        let bg = theme::current().palette.background;
        let mut win = AppWindow::new("t", 400, 300);
        let mut body = win.body();
        let card = View::new(200, 80).paint(|c, u| c.fill(&gfx::Path::rounded_rect(0.0, 0.0, 200.0, 80.0, [24.0; 4]), u.accent));
        body.fit(&*card);
        // A slow shader on a small area budget: renders at lower resolution, in several draws.
        let slow = View::new(360, 120)
            .cpu_shader(|x: f32, y: f32, u: &gfx::Uniforms| { let mut v = 0.0f32; for i in 0..300 { v += ((x + y + i as f32) * 0.01).sin(); } u.accent.mix(u.background, (v * 0.0).abs()) })
            .max_area(20_000)
            .budget_ms(1.0);
        body.fit(&*slow);
        win.end();
        let p = shots.join("gfx-view.png");
        win.snapshot(&p).map_err(|e| e.to_string())?;
        let mid = pixel(&p, card.x() + 100, card.y() + 40)?;
        let corner = pixel(&p, card.x() + 1, card.y() + 1)?;
        check(mid == [acc.r(), acc.g(), acc.b()], &format!("card center {mid:?}"))?;
        check(corner == [bg.r(), bg.g(), bg.b()], &format!("rounded corner shows the window behind: {corner:?}"))?;
        let st = slow.stats();
        check(st.frames == 1 && st.render_size.0 * st.render_size.1 <= 20_000 && st.render_size.0 < 360, &format!("first frame complete at reduced size: {st:?}"))?;
        let sp = pixel(&p, slow.x() + 180, slow.y() + 60)?;
        check(sp == [acc.r(), acc.g(), acc.b()], &format!("scaled shader output {sp:?}"))?;
        slow.invalidate();
        let mut draws = 0;
        while slow.stats().frames < 2 && draws < 200 { win.snapshot(std::env::temp_dir().join("omarchy-fltk-unused.png")).ok(); draws += 1; }
        let st = slow.stats();
        check(st.frames == 2 && st.partial_draws > 0, &format!("next frame spread over {draws} draws within a 1 ms budget: {st:?}"))?;
        check(st.memory > 0 && st.memory < 400_000, &format!("buffers bounded by max_area: {} bytes", st.memory))?;

        // Hit shape: a round button over a rectangular one.
        let mut win = AppWindow::new("t", 300, 200);
        win.end();
        let behind = Rc::new(RefCell::new(0));
        let b2 = behind.clone();
        let mut under = fltk::button::Button::new(20, 60, 120, 120, None);
        under.set_callback(move |_| *b2.borrow_mut() += 1);
        win.add(&under);
        let round = Rc::new(RefCell::new(0));
        let r2 = round.clone();
        let mut orb = View::new(120, 120)
            .paint(|c, u| c.fill(&gfx::Path::circle(60.0, 60.0, 60.0), u.accent))
            .hit_shape(Hit::Fill(gfx::Path::circle(60.0, 60.0, 60.0)))
            .on_click(move || *r2.borrow_mut() += 1);
        orb.set_pos(20, 60);
        win.add(&*orb);
        focus::click_at(&*win, 80, 120); // center of the circle
        focus::click_at(&*win, 23, 63);  // bounding-box corner, outside the circle
        check(*round.borrow() == 1 && *behind.borrow() == 1, &format!("circle got its click, the corner went below: {} / {}", round.borrow(), behind.borrow()))?;
        let mut u2 = under.clone();
        let _ = u2.take_focus();
        press("Tab");
        check(focus::is_focused(&*orb), "the shaped view is a Tab stop when clickable")?;
        press("Enter");
        check(*round.borrow() == 2, "Enter activates it")?;
        win.snapshot(shots.join("gfx-hit-focus.png")).map_err(|e| e.to_string())?;

        // Animation ticks are skipped while the window is not shown.
        let anim = View::new(50, 50).shader(gfx::shaders::noise()).animate(60);
        let mut w3 = AppWindow::new("t", 100, 100);
        w3.add(&*anim);
        w3.end();
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_millis(1200) { App::pump(); std::thread::sleep(Duration::from_millis(10)); }
        let st = anim.stats();
        check(st.paused_ticks >= 1 && st.frames == 0, &format!("hidden: paused, no frames: {st:?}"))?;

        // Theme change repaints with the new accent.
        let mut t = theme::current();
        t.palette.accent = theme::Rgb(0x33ccff);
        theme::set(t);
        let p = shots.join("gfx-view-theme.png");
        let mut w4 = AppWindow::new("t", 300, 140);
        let mut b4 = w4.body();
        let v4 = View::new(100, 40).paint(|c, u| c.fill(&gfx::Path::rect(0.0, 0.0, 100.0, 40.0), u.accent));
        b4.fit(&*v4);
        w4.end();
        w4.snapshot(&p).map_err(|e| e.to_string())?;
        let px = pixel(&p, v4.x() + 50, v4.y() + 20)?;
        theme::reload();
        check(px == [0x33, 0xcc, 0xff], &format!("theme color reaches the view: {px:?}"))
    });
    run("label bound before fit keeps its text width", &|_| {
        let mut win = AppWindow::new("t", 300, 100);
        let mut r = layout::row();
        let n = State::new(64u32);
        let l = Label::new("").bind(&n.map(|v| format!("{v}% volume")));
        r.fit(&*l);
        r.close_fitted();
        win.end();
        win.snapshot(std::env::temp_dir().join("omarchy-fltk-unused.png")).ok();
        check(l.w() > 40, &format!("width {}", l.w()))
    });
    run("transitions: Omarchy durations, intermediate colors, timer stops, can be turned off", &|shots| {
        fltk_omarchy::anim::set_enabled(true);
        let mut win = AppWindow::new("t", 300, 140);
        let mut body = win.body();
        let b = Button::new("Save").bordered();
        body.fit(&*b);
        let on = State::new(false);
        let sw = ToggleSwitch::new(&on);
        body.fit(&*sw);
        let field = TextField::new();
        body.fit(&*field);
        win.end();
        let shot = |name: &str| -> Result<PathBuf, String> {
            let p = shots.join(name);
            let mut w = win.clone();
            w.snapshot(&p).map_err(|e| e.to_string())?;
            Ok(p)
        };
        shot("anim-0.png")?;
        let edge = |p: &Path| pixel(p, b.x(), b.y() + b.h() / 2);
        let before = edge(&shot("anim-1.png")?)?;
        let mut s0 = sw.clone();
        let _ = s0.take_focus();
        press("Shift+Tab"); // keyboard focus on the button: border goes to the accent ring
        check(focus::is_focused(&*b), "button focused")?;
        let t0 = Instant::now();
        let first = edge(&shot("anim-2.png")?)?;
        while t0.elapsed() < Duration::from_millis(60) { App::pump(); std::thread::sleep(Duration::from_millis(5)); }
        let mid = edge(&shot("anim-3-mid.png")?)?;
        let running = fltk_omarchy::anim::pending() > 0;
        while t0.elapsed() < Duration::from_millis(250) { App::pump(); std::thread::sleep(Duration::from_millis(5)); }
        let end = edge(&shot("anim-4-end.png")?)?;
        App::pump();
        let acc = theme::current().palette.accent;
        let target = [acc.r(), acc.g(), acc.b()];
        let dist = |a: [u8; 3], b: [u8; 3]| (0..3).map(|i| (a[i] as i32 - b[i] as i32).abs()).sum::<i32>();
        check(first == before || dist(first, target) > dist(mid, target), &format!("starts at the old color: {first:?}"))?;
        check(dist(mid, target) > 0 && dist(mid, target) < dist(before, target), &format!("halfway at 60 ms of 120: {before:?} → {mid:?} → {end:?}"))?;
        check(end == target && running, &format!("ends on the ring color, timer ran: {end:?}"))?;
        // Switch knob moves through intermediate positions (120 ms OutCubic).
        let knob_x = |p: &Path| -> Result<i32, String> {
            let ky = sw.y() + sw.h() / 2;
            let fg = theme::current().palette.foreground.darker(1.25);
            let sel = theme::current().controls.selected.color;
            let mut xs = Vec::new();
            for x in sw.x()..sw.x() + sw.w() {
                let c = pixel(p, x, ky)?;
                if [fg, sel].iter().any(|k| dist(c, [k.r(), k.g(), k.b()]) < 40) { xs.push(x); }
            }
            Ok(xs.first().copied().unwrap_or(-1))
        };
        let x_off = knob_x(&shot("sw-0.png")?)?;
        on.set(true);
        let t1 = Instant::now();
        shot("sw-1.png")?;
        while t1.elapsed() < Duration::from_millis(40) { App::pump(); std::thread::sleep(Duration::from_millis(5)); }
        let x_mid = knob_x(&shot("sw-2-mid.png")?)?;
        while t1.elapsed() < Duration::from_millis(250) { App::pump(); std::thread::sleep(Duration::from_millis(5)); }
        let x_on = knob_x(&shot("sw-3-end.png")?)?;
        check(x_off < x_mid && x_mid < x_on, &format!("knob x {x_off} → {x_mid} → {x_on}"))?;
        // Off: the change is immediate.
        fltk_omarchy::anim::set_enabled(false);
        on.set(false);
        let x_now = knob_x(&shot("sw-4-off.png")?)?;
        fltk_omarchy::anim::set_enabled(true);
        check(x_now == x_off && fltk_omarchy::anim::pending() == 0, &format!("disabled: jumps to {x_now}"))
    });
    run("portable shaders: same pixels as sl eval, backend choice, display rate on the CPU", &|shots| {
        let sh = gfx::shaders::gradient();
        let mut win = AppWindow::new("t", 400, 260);
        let mut body = win.body();
        let v = View::new(300, 150).shader(sh.clone());
        v.set_time(Some(0.7));
        body.fit(&*v);
        win.end();
        let p = shots.join("sl-view.png");
        win.snapshot(&p).map_err(|e| e.to_string())?;
        let mut u = gfx::Uniforms::from_theme(&theme::current(), (v.w() as f32, v.h() as f32));
        u.time = 0.7;
        for (x, y) in [(10, 10), (150, 75), (290, 140)] {
            let want = sh.eval(x as f32 + 0.5, y as f32 + 0.5, &u).to_u8();
            let got = pixel(&p, v.x() + x, v.y() + y)?;
            check((0..3).all(|i| (want[i] as i32 - got[i] as i32).abs() <= 2), &format!("({x},{y}) view {got:?} vs eval {want:?}"))?;
        }
        // Backend: without the gpu feature the View stays on the CPU and says why.
        let anim = View::new(60, 40).shader(gfx::shaders::rings()).animate(gfx::Rate::Display);
        let fixed = View::new(60, 40).shader(gfx::shaders::rings()).animate(50);
        let mut w2 = AppWindow::new("t", 200, 120);
        w2.add(&*anim);
        w2.add(&*fixed);
        w2.end();
        pump_until(|| anim.stats().target_fps > 0.0 && fixed.stats().target_fps > 0.0);
        check(anim.active_backend() == gfx::Backend::Cpu, "CPU backend")?;
        if !cfg!(feature = "gpu") {
            check(anim.backend_reason().contains("gpu"), &format!("reason: {:?}", anim.backend_reason()))?;
        }
        // No Hyprland in tests: display rate falls back to 60 Hz, capped at 30 on the CPU.
        check(anim.stats().target_fps == 30.0 && fixed.stats().target_fps == 50.0, &format!("fps {} / {}", anim.stats().target_fps, fixed.stats().target_fps))?;
        gfx::set_cpu_fps_cap(24);
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_millis(700) { App::pump(); std::thread::sleep(Duration::from_millis(10)); }
        let capped = anim.stats().target_fps;
        gfx::set_cpu_fps_cap(30);
        check(capped == 24.0, &format!("cap applies: {capped}"))?;
        let forced = View::new(10, 10).shader(gfx::shaders::rings()).backend(gfx::Backend::Cpu);
        check(forced.active_backend() == gfx::Backend::Cpu, "forced CPU")
    });
    run("issue #6: app shortcuts reach the window while a popup is open; j/k only unmodified", &|shots| {
        let mut win = AppWindow::new("t", 420, 320);
        let mut body = win.body();
        let anchor = Button::new("Menu");
        body.fit(&*anchor);
        let dv = State::new(0usize);
        let mut dd = Dropdown::new("Output", &["A", "B", "C"], &dv);
        body.fit(&*dd);
        let sv = State::new(None::<usize>);
        let opts = vec![("Home".to_string(), "~/".to_string()), ("Work".to_string(), "~/Work".to_string())];
        let mut sd = SearchableDropdown::new("Go to", opts, &sv);
        body.fit(&*sd);
        win.end();
        win.snapshot(std::env::temp_dir().join("omarchy-fltk-unused.png")).ok();
        let hits = Rc::new(RefCell::new(Vec::<&'static str>::new()));
        for spec in ["F1", "F10", "Ctrl+,", "Ctrl+j", "Alt+k", "Ctrl+Down"] {
            let h2 = hits.clone();
            win.shortcut(spec, move || h2.borrow_mut().push(spec));
        }
        let layers = win.children();
        let items = || vec![PopupItem::new("", "One"), PopupItem::new("", "Two"), PopupItem::new("", "Three")];
        let open = |picks: &Rc<RefCell<Vec<usize>>>, seen: &Rc<RefCell<Vec<Dismiss>>>| {
            let mut a = anchor.clone();
            let _ = a.take_focus();
            let (p, s) = (picks.clone(), seen.clone());
            PopupMenu::open(&*anchor, items(), move |i| p.borrow_mut().push(i)).on_dismiss(move |d| s.borrow_mut().push(d))
        };
        let picks = Rc::new(RefCell::new(Vec::new()));
        let seen = Rc::new(RefCell::new(Vec::new()));

        // Every global key: the menu closes with Closed, focus comes back, the
        // window shortcut runs exactly once, nothing is picked.
        for spec in ["F1", "F10", "Ctrl+,", "Ctrl+j", "Alt+k", "Ctrl+Down"] {
            hits.borrow_mut().clear();
            seen.borrow_mut().clear();
            let h = open(&picks, &seen);
            check(h.is_open() && !focus::is_focused(&*anchor), &format!("{spec}: menu open with the keyboard"))?;
            press(spec);
            check(*hits.borrow() == vec![spec], &format!("{spec}: window shortcut ran once: {:?}", hits.borrow()))?;
            check(!h.is_open() && h.dismissed() == Some(Dismiss::Closed) && *seen.borrow() == vec![Dismiss::Closed],
                  &format!("{spec}: dismissed with Closed: {:?}", seen.borrow()))?;
            check(focus::is_focused(&*anchor) && win.children() == layers && picks.borrow().is_empty(),
                  &format!("{spec}: focus back, layer gone, no pick"))?;
        }

        // Navigation still works: plain j/k, arrows, Tab stays, Enter picks.
        let h = open(&picks, &seen);
        press("j"); press("j"); press("k");
        press("Tab");
        check(h.is_open() && !focus::is_focused(&*anchor), "Tab stays in the open menu")?;
        win.snapshot(shots.join("issue6-menu.png")).map_err(|e| e.to_string())?;
        press("Down"); press("Enter");
        check(*picks.borrow() == vec![2] && h.dismissed() == Some(Dismiss::Picked(2)) && focus::is_focused(&*anchor),
              &format!("j, j, k, Down, Enter picks Three: {:?}", picks.borrow()))?;
        let h = open(&picks, &seen);
        press("Escape");
        check(h.dismissed() == Some(Dismiss::Escape) && focus::is_focused(&*anchor), "Esc keeps its reason")?;
        check(hits.borrow().len() == 1, "Esc and navigation keys reach no window shortcut")?;

        // Dropdown popup: Ctrl+j is not navigation; it closes and reaches the app.
        hits.borrow_mut().clear();
        dd.open();
        press("Ctrl+j");
        pump_until(|| win.children() == layers);
        check(*hits.borrow() == vec!["Ctrl+j"] && dv.get() == 0 && win.children() == layers && focus::is_focused(&*dd),
              &format!("dropdown: Ctrl+j → app, value kept, closed, focus back: {:?} {}", hits.borrow(), dv.get()))?;
        dd.open();
        press("j"); press("Enter");
        pump_until(|| win.children() == layers);
        check(dv.get() == 1, "dropdown: plain j still navigates")?;
        dd.open();
        press("F10");
        pump_until(|| win.children() == layers);
        check(hits.borrow().last() == Some(&"F10") && win.children() == layers, "dropdown: F10 reaches the app")?;

        // SearchableDropdown: typing filters; F1 and Ctrl+, close and reach the app.
        hits.borrow_mut().clear();
        sd.open();
        press("w");
        check(win.children() == layers + 1, "searchable: typing keeps it open")?;
        press("Ctrl+,");
        check(*hits.borrow() == vec!["Ctrl+,"] && win.children() == layers && sv.get().is_none(), "searchable: Ctrl+, → app")?;
        sd.open();
        press("F1");
        check(hits.borrow().last() == Some(&"F1") && win.children() == layers, "searchable: F1 → app")?;

        // Menu (type to filter) and ShortcutHelp follow the same rule.
        hits.borrow_mut().clear();
        let mut a = anchor.clone();
        let _ = a.take_focus();
        Menu::open(&*win, "Go", vec![MenuItem::new("", "Apps"), MenuItem::new("", "System")], |_| {});
        press("a");
        check(win.children() == layers + 1 && hits.borrow().is_empty(), "menu: typing filters")?;
        press("F10");
        check(*hits.borrow() == vec!["F10"] && win.children() == layers && focus::is_focused(&*anchor), "menu: F10 → app, focus back")?;
        let _help = ShortcutHelp::open(&*win, "Keys", vec![("App".into(), vec![("F10".into(), "Menu".into())])]);
        press("Ctrl+,");
        check(hits.borrow().last() == Some(&"Ctrl+,") && win.children() == layers, "help: Ctrl+, → app")?;
        let _help = ShortcutHelp::open(&*win, "Keys", vec![("App".into(), vec![("F10".into(), "Menu".into())])]);
        let n = hits.borrow().len();
        press("F1");
        check(hits.borrow().len() == n && win.children() == layers, "help: F1 still just closes the help")?;

        // ConfirmDialog is a decision: it keeps the keyboard.
        ConfirmDialog::open(&*win, "Sure?", "Yes", false, |_| {});
        press("F10");
        check(hits.borrow().len() == n && win.children() == layers + 1, "confirm dialog keeps F10")?;
        press("Escape");
        check(win.children() == layers, "dialog closed")
    });
    run("issue #7: keyboard panel focus events are synchronous, ordered and carry the panel", &|shots| {
        use fltk_omarchy::focus::{PanelFocus, PanelFocusCause as C};
        let mut win = AppWindow::new("t", 640, 360);
        let mut body = win.body();
        let field = TextField::new().placeholder("Path");
        body.fit(&*field);
        let left = KeyboardPanel::new("left");
        let right = KeyboardPanel::new("right");
        left.set_items(5, |i| i as u64, |id| Some(id as usize));
        right.set_items(5, |i| 100 + i as u64, |id| id.checked_sub(100).map(|x| x as usize));
        let mut canvas = fltk::widget::Widget::default().with_size(600, 200);
        canvas.handle(|_, ev| matches!(ev, fltk::enums::Event::Focus | fltk::enums::Event::Unfocus));
        body.fit(&canvas);
        focus::host(&canvas, &[&left, &right]);
        let after = Button::new("After");
        body.fit(&*after);
        win.end();
        win.snapshot(std::env::temp_dir().join("omarchy-fltk-unused.png")).ok();

        let name = |p: &Option<KeyboardPanel>| p.as_ref().map_or("-".to_string(), |p| p.name());
        let log = Rc::new(RefCell::new(Vec::<String>::new()));
        // The app's model: the active pane, updated only from the event.
        let model = Rc::new(RefCell::new(String::new()));
        let (l2, m2, c2) = (log.clone(), model.clone(), canvas.clone());
        let listener = focus::on_panel_focus(move |e: &PanelFocus| {
            // At notification time the router state already matches the event,
            // and the host repaint is still pending.
            let consistent = focus::focused_panel() == e.current;
            let pending = e.current.is_none() || c2.damage();
            l2.borrow_mut().push(format!("{}>{} {:?}{}", name(&e.previous), name(&e.current), e.cause,
                                         if consistent && pending { "" } else { " INCONSISTENT" }));
            if let Some(p) = &e.current { *m2.borrow_mut() = p.name(); }
        });
        let per_left = Rc::new(RefCell::new(Vec::new()));
        let pl = per_left.clone();
        let l3 = log.clone();
        left.on_focus(move |f| { pl.borrow_mut().push(f); l3.borrow_mut().push(format!("left.on_focus({f})")); });
        // The next key a panel sees is logged too: ordering against events.
        let l4 = log.clone();
        right.on_key(move |k, _| { if k == fltk::enums::Key::from_char('x') { l4.borrow_mut().push("right got x".into()); true } else { false } });

        let mut f2 = field.input();
        let _ = f2.take_focus();
        App::pump();
        log.borrow_mut().clear();
        let take = |log: &Rc<RefCell<Vec<String>>>| std::mem::take(&mut *log.borrow_mut());
        let expect = |want: &[&str], what: &str| -> Result<(), String> {
            let got = take(&log);
            check(got == want, &format!("{what}: got {got:?}"))
        };

        press("Tab");
        expect(&["left.on_focus(true)", "->left Tab"], "Tab into the left pane, before press() returns")?;
        check(*model.borrow() == "left", "the model follows the event")?;
        press("Tab");
        press("x");
        expect(&["left.on_focus(false)", "left>right Tab", "right got x"], "Tab left → right, reported before the next key reaches the panel")?;
        win.snapshot(shots.join("issue7-right.png")).map_err(|e| e.to_string())?;
        press("Shift+Tab");
        expect(&["left.on_focus(true)", "right>left ShiftTab"], "Shift+Tab right → left")?;
        press("Shift+Tab");
        expect(&["left.on_focus(false)", "left>- ShiftTab"], "Shift+Tab out of the panes")?;
        check(focus::current().is_some_and(|f| f.as_widget_ptr() == field.input().as_widget_ptr()),
              "Shift+Tab out to the path field: current is None")?;
        check(focus::active_panel_of(&canvas) == Some(left.clone()), "the host keeps left as its active pane")?;

        right.focus();
        expect(&["->right Focus"], "KeyboardPanel::focus")?;
        // Host replacement drops the focused pane.
        focus::host(&canvas, &[&left]);
        expect(&["left.on_focus(true)", "right>left HostChanged"], "host replaced: right → left")?;
        focus::host(&canvas, &[&left, &right]);
        check(take(&log).is_empty(), "host restored with left still active: no event")?;

        // Overlays: focus leaves, then comes back with Restored.
        let h = PopupMenu::open(&canvas, vec![PopupItem::new("", "Copy")], |_| {});
        expect(&["left.on_focus(false)", "left>- Overlay"], "menu takes focus")?;
        press("Escape");
        expect(&["left.on_focus(true)", "->left Restored"], "Esc gives focus back to the pane")?;
        check(h.dismissed() == Some(Dismiss::Escape),
              &format!("Esc gives focus back to the pane: {:?}", log.borrow()))?;
        // An F-key closing a menu (issue #6) restores the pane too.
        let h = PopupMenu::open(&canvas, vec![PopupItem::new("", "Copy")], |_| {});
        expect(&["left.on_focus(false)", "left>- Overlay"], "menu again")?;
        press("F5");
        expect(&["left.on_focus(true)", "->left Restored"], "F5 closes the menu, pane restored")?;
        check(h.dismissed() == Some(Dismiss::Closed), "F5: Closed")?;

        // A click on an ordinary control.
        focus::click_at(&*win, after.x() + 5, after.y() + 5);
        expect(&["left.on_focus(false)", "left>- Other"], "click on a button")?;

        // Focus moved by app code outside any event: reported at the next check.
        let mut c3 = canvas.clone();
        let _ = c3.take_focus();
        check(take(&log).is_empty(), "not yet: no event ran")?;
        App::pump();
        expect(&["left.on_focus(true)", "->left Other"], "reported before the loop waits / paints")?;

        // A listener that moves focus: reported next, in order, no recursion.
        let r2 = right.clone();
        let once = Rc::new(Cell::new(true));
        let bounce = focus::on_panel_focus(move |e| {
            if e.cause == C::ShiftTab && once.replace(false) { r2.focus(); }
        });
        press("Shift+Tab");
        expect(&["left.on_focus(false)", "left>- ShiftTab", "->right Focus"], "re-entrant focus change reported after the first")?;
        focus::remove_panel_focus(bounce);

        // Lifetime: the window goes away while a pane has focus.
        let keep = right.clone();
        fltk::window::Window::delete((*win).clone());
        App::pump();
        expect(&["right>- Other"], "host deleted")?;
        check(keep.name() == "right", "host deleted: reported once, the panel handle stays valid")?;
        focus::remove_panel_focus(listener);
        let pl = per_left.borrow().clone();
        check(pl.iter().filter(|f| **f).count() == pl.iter().filter(|f| !**f).count(), &format!("left: every gain has a loss: {pl:?}"))
    });
    run("file manager pieces: overflowing tabs, bounded help, localized toast, popup card", &|shots| {
        // Tabs wider than the bar: arrows, scrolling, the selected tab in view.
        let mut win = AppWindow::new("t", 360, 300);
        let mut body = win.body();
        let tab = State::new(0usize);
        let names: Vec<String> = (1..=8).map(|i| format!("Folder number {i}")).collect();
        let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        let tabs = TabBar::new(&refs, &tab).on_close(|_| {});
        body.fit(&*tabs);
        win.end();
        win.snapshot(shots.join("tabs-0.png")).map_err(|e| e.to_string())?;
        check(tabs.overflowing() && tabs.tab_rect(7).is_none(), "8 tabs overflow; the last one starts out of view")?;
        let full = tabs.tab_rect(0).ok_or("first tab")?.1;
        let mut t2 = tabs.clone();
        let _ = t2.take_focus();
        press("End");
        win.snapshot(shots.join("tabs-1-end.png")).map_err(|e| e.to_string())?;
        let r = tabs.tab_rect(7).ok_or("last tab visible")?;
        check(tab.get() == 7 && r.1 >= full && tabs.scroll_offset() > 0, &format!("selected tab fully visible (close glyph included): {r:?}"))?;
        let s0 = tabs.scroll_offset();
        focus::click_at(&*win, tabs.x() + 5, tabs.y() + tabs.h() / 2); // left arrow
        check(tabs.scroll_offset() < s0 && tab.get() == 7, "left arrow scrolls without changing the tab")?;

        // Shortcut help: bounded scrolling, Home/End, large font.
        let user = PathBuf::from(std::env::var("OMARCHY_FLTK_USER_SHELL").unwrap());
        std::fs::write(&user, "[font]\nbase-size = 18\n").unwrap();
        theme::reload();
        let mut w2 = AppWindow::new("t", 420, 320);
        w2.end();
        w2.snapshot(std::env::temp_dir().join("omarchy-fltk-unused.png")).ok();
        let row = |k: &str, d: &str| (k.to_string(), d.to_string());
        let sections: Vec<(String, Vec<(String, String)>)> = (0..6).map(|i| (format!("Section {i}"), (0..5).map(|j| row("Ctrl+Shift+F12", &format!("Action {i}.{j}"))).collect())).collect();
        let help = ShortcutHelp::open(&*w2, "Keyboard shortcuts", sections);
        w2.snapshot(shots.join("help-0.png")).map_err(|e| e.to_string())?;
        press("End");
        w2.snapshot(shots.join("help-1-end.png")).map_err(|e| e.to_string())?;
        let (end, max) = (help.scroll(), help.max_scroll());
        for _ in 0..5 { press("Down"); }
        w2.snapshot(std::env::temp_dir().join("omarchy-fltk-unused.png")).ok();
        let after = help.scroll();
        press("Home");
        w2.snapshot(std::env::temp_dir().join("omarchy-fltk-unused.png")).ok();
        check(max > 0 && end == max && after == max && help.scroll() == 0, &format!("End → {end} of {max}, stays at {after}, Home → {}", help.scroll()))?;
        press("Escape");
        std::fs::remove_file(&user).unwrap();
        theme::reload();

        // Operation toast: localized labels, indeterminate, follows the window size.
        let mut w3 = AppWindow::new("t", 520, 360);
        w3.end();
        let events = Rc::new(RefCell::new(Vec::new()));
        let (e1, e2) = (events.clone(), events.clone());
        let mut op = OperationToast::show(&*w3, "Copiando 3 archivos").ok_or("toast")?
            .labels("Pausar", "Reanudar", "Cancelar", "En pausa")
            .on_pause(move |p| e1.borrow_mut().push(format!("pause {p}")))
            .on_cancel(move || e2.borrow_mut().push("cancel".to_string()));
        op.set_indeterminate(true);
        op.set_detail("informe.pdf");
        w3.snapshot(shots.join("toast-es.png")).map_err(|e| e.to_string())?;
        op.toggle_pause();
        op.cancel();
        let mut w3b = w3.clone();
        w3b.resize(w3.x(), w3.y(), 400, 260);
        w3.snapshot(std::env::temp_dir().join("omarchy-fltk-unused.png")).ok();
        w3.snapshot(shots.join("toast-resized.png")).map_err(|e| e.to_string())?;
        let (x, y, w, h) = op.rect();
        check(x + w <= 400 && y + h <= 260 && x > 0 && y > 0, &format!("toast follows the corner: {:?}", op.rect()))?;
        check(*events.borrow() == vec!["pause true".to_string(), "cancel".to_string()], &format!("{:?}", events.borrow()))?;
        op.close();

        // Popup card: widgets inside, Tab stays in, Esc closes, focus comes back.
        let mut w4 = AppWindow::new("t", 420, 300);
        let mut b4 = w4.body();
        let anchor = Button::new("Volume");
        b4.fit(&*anchor);
        let after = Button::new("After");
        b4.fit(&*after);
        w4.end();
        w4.snapshot(std::env::temp_dir().join("omarchy-fltk-unused.png")).ok();
        let mut a4 = anchor.clone();
        let _ = a4.take_focus();
        let vol = State::new(0.5);
        let inner_btn: Rc<RefCell<Option<Button>>> = Rc::default();
        let ib = inner_btn.clone();
        let v2 = vol.clone();
        let card = PopupCard::open(&*anchor, 260, 120, move |col| {
            col.fit(&*Label::section("Output"));
            let _s = Slider::new(&v2, 0.0, 1.0, 0.1);
            let b = Button::new("Mute");
            col.fit(&*b);
            *ib.borrow_mut() = Some(b);
        });
        w4.snapshot(shots.join("card.png")).map_err(|e| e.to_string())?;
        let rect = card.rect().ok_or("rect")?;
        check(rect.0 >= 0 && rect.1 + rect.3 <= 300, &format!("card in the window: {rect:?}"))?;
        press("Right");
        check((vol.get() - 0.6).abs() < 1e-9, &format!("first control (slider) has focus: {}", vol.get()))?;
        press("Tab");
        let mute = inner_btn.borrow().clone().ok_or("mute")?;
        check(focus::is_focused(&*mute), "Tab to the card's button")?;
        press("Tab");
        check(!focus::is_focused(&*after) && !focus::is_focused(&*anchor), "Tab stays inside the card")?;
        press("Escape");
        check(card.dismissed() == Some(Dismiss::Escape) && focus::is_focused(&*anchor), "Esc closes, focus back on the anchor")?;
        let card2 = PopupCard::open(&*anchor, 200, 80, |col| { col.fit(&*Label::new("Hi")); });
        focus::click_at(&*w4, 410, 290);
        check(card2.dismissed() == Some(Dismiss::ClickOutside), "click outside closes")
    });
    run("virtual table and tree on keyboard panels; native gesture guard", &|shots| {
        let mut win = AppWindow::new("t", 520, 360);
        let mut body = win.body();
        let n = 10_000usize;
        let panel = KeyboardPanel::new("files");
        panel.set_items(n, |i| 7_000 + i as u64, move |id| id.checked_sub(7_000).map(|x| x as usize).filter(|x| *x < n));
        let sorts = Rc::new(RefCell::new(Vec::new()));
        let s2 = sorts.clone();
        let table = Table::new(&panel, vec![Column::new("Name", 0).flex(), Column::new("Size", 90).right(), Column::new("Kind", 80)],
            |row, col| match col { 0 => format!("file-{row}.txt"), 1 => format!("{} KB", row * 3), _ => "Text".into() })
            .on_sort(move |c, asc| s2.borrow_mut().push((c, asc)));
        body.fit(&*table);
        let after = Button::new("After");
        body.fit(&*after);
        win.end();
        win.snapshot(shots.join("table-0.png")).map_err(|e| e.to_string())?;
        check(table.rows_drawn() <= 20, &format!("virtual: {} rows drawn of {n}", table.rows_drawn()))?;
        focus::first(&*win);
        check(panel.has_focus(), "Tab stop: the table's panel")?;
        press("Down"); press("Down"); press("Down"); press("Shift+Down"); press("Shift+Down");
        check(panel.selection() == vec![7_002, 7_003, 7_004], &format!("range by ID: {:?}", panel.selection()))?;
        press("End");
        win.snapshot(shots.join("table-1-end.png")).map_err(|e| e.to_string())?;
        check(panel.cursor() == Some(7_000 + n as u64 - 1) && table.scroll_offset() > 0, "End scrolls to the last row")?;
        let cw = table.column_widths();
        let size_x = table.x() + cw[0] + cw[1] / 2; // middle of the "Size" header
        focus::click_at(&*win, size_x, table.y() + 8);
        focus::click_at(&*win, size_x, table.y() + 8);
        check(*sorts.borrow() == vec![(1, true), (1, false)] && table.sort() == Some((1, false)), &format!("header sort: {:?}", sorts.borrow()))?;
        win.snapshot(shots.join("table-2-sorted.png")).map_err(|e| e.to_string())?;
        press("Tab");
        check(focus::is_focused(&*after), "Tab leaves the table")?;

        // Tree: lazy children, Right/Left, chevron click.
        let calls = Rc::new(RefCell::new(Vec::new()));
        let c2 = calls.clone();
        let tp = KeyboardPanel::new("tree");
        let mut w2 = AppWindow::new("t", 360, 320);
        let mut b2 = w2.body();
        let tree = Tree::new(&tp, move |parent| {
            c2.borrow_mut().push(parent);
            match parent {
                None => (0..3).map(|i| TreeNode::new(i, &format!("Root {i}")).icon(glyph::FOLDER).branch()).collect(),
                Some(p) if p < 3 => (0..1000).map(|j| TreeNode::new(100 + p * 1000 + j, &format!("Child {p}.{j}")).icon(glyph::FILE)).collect(),
                _ => vec![],
            }
        });
        b2.fit(&*tree);
        w2.end();
        w2.snapshot(std::env::temp_dir().join("omarchy-fltk-unused.png")).ok();
        check(tree.visible_len() == 3 && *calls.borrow() == vec![None], "only roots asked for")?;
        let mut tw = tree.clone();
        let _ = tw.take_focus();
        press("Down"); press("Right");
        check(tree.is_expanded(0) && tree.visible_len() == 1003 && calls.borrow().contains(&Some(0)), "Right expands, children asked lazily")?;
        press("Right");
        check(tp.cursor() == Some(100), "Right again: first child")?;
        press("Left");
        check(tp.cursor() == Some(0), "Left: back to the parent")?;
        press("Left");
        check(!tree.is_expanded(0) && tree.visible_len() == 3, "Left again: collapse")?;
        let asked = calls.borrow().len();
        tree.expand(0);
        tree.expand(1);
        check(calls.borrow().len() == asked + 1, &format!("children are cached: re-expanding asks only for the new node ({:?})", &calls.borrow()[asked..]))?;
        tree.collapse(1);
        tree.collapse(0);
        press("Down"); press("Right"); press("Right"); press("Shift+Down"); press("Shift+Down");
        w2.snapshot(shots.join("tree.png")).map_err(|e| e.to_string())?;
        check(tp.selection() == vec![1_100, 1_101, 1_102] && tree.rows_drawn() < 80, &format!("selection in a 1000-child branch: {:?}, drawn {}", tp.selection(), tree.rows_drawn()))?;

        // Native move/resize: never on a window that is not shown or outside a button press.
        check(!fltk_omarchy::native::available(&*win) && !fltk_omarchy::native::begin_move(&*win), "native gesture refuses when not possible")
    });
    run("real clicks (fltk event path) on every clickable control: no re-entrancy panic", &|_| {
        let mut win = AppWindow::new("t", 520, 420);
        let mut body = win.body();
        let n = State::new(0u32);
        let c = n.clone();
        let mut r = layout::row();
        let b = Button::new("Click").on_click(move || c.update(|v| *v += 1));
        r.fit(&*b);
        let sw_on = State::new(false);
        let sw = ToggleSwitch::new(&sw_on);
        r.fit(&*sw);
        r.close_fitted();
        let t_on = State::new(false);
        let tg = Toggle::new("Night light", "", &t_on);
        body.fit(&*tg);
        let rc = Rc::new(RefCell::new(0));
        let r2 = rc.clone();
        let row = Row::new("", "Row").on_click(move || *r2.borrow_mut() += 1);
        body.fit(&*row);
        let vol = State::new(0.0);
        let _sl = Slider::new(&vol, 0.0, 1.0, 0.1);
        let tab = State::new(0usize);
        let tabs = TabBar::new(&["A", "B"], &tab);
        body.fit(&*tabs);
        win.end();
        win.snapshot(std::env::temp_dir().join("omarchy-fltk-unused.png")).ok();
        let mid = |w: &dyn WidgetExt| (w.x() + w.w() / 2, w.y() + w.h() / 2);
        let (x, y) = mid(&*b); focus::click_at(&*win, x, y); focus::click_at(&*win, x, y);
        let (x, y) = mid(&*sw); focus::click_at(&*win, x, y);
        let (x, y) = mid(&*tg); focus::click_at(&*win, x, y);
        let (x, y) = mid(&*row); focus::click_at(&*win, x, y);
        focus::click_at(&*win, _sl.x() + _sl.w() - 2, _sl.y() + _sl.h() / 2);
        focus::click_at(&*win, tabs.x() + 100, tabs.y() + tabs.h() / 2);
        check(n.get() == 2 && sw_on.get() && t_on.get() && *rc.borrow() == 1 && vol.get() > 0.8 && tab.get() == 1,
              &format!("clicks {} switch {} toggle {} row {} slider {} tab {}", n.get(), sw_on.get(), t_on.get(), rc.borrow(), vol.get(), tab.get()))
    });
    run("keyboard only: Tab order, arrows, shortcuts, focus comes back", &|shots| {
        let mut win = AppWindow::new("t", 560, 420);
        let mut body = win.body();
        let name = State::new(String::new());
        let field = TextField::new().placeholder("Name").bind(&name);
        body.fit(&*field);
        let clicks = State::new(0u32);
        let c = clicks.clone();
        let mut r = layout::row();
        let save = Button::new("Save").on_click(move || c.update(|v| *v += 1));
        r.fit(&*save);
        let view = State::new(0usize);
        let group = ButtonGroup::new(&["Day", "Week", "Month"], &view);
        r.fit(&*group);
        r.close_fitted();
        let tab = State::new(0usize);
        let tabs = TabBar::new(&["Home", "Work", "Music"], &tab);
        body.fit(&*tabs);
        let sel = State::new(None::<usize>);
        let opened = Rc::new(RefCell::new(None));
        let o2 = opened.clone();
        let list = List::new(&State::new(50usize), |i| ListRow::new(&format!("Item {i}"))).bind_selected(&sel)
            .on_activate(move |i| *o2.borrow_mut() = Some(i));
        win.end();
        let k = |spec: &str| { press(spec); };

        focus::first(&*win).ok_or("nothing focusable")?;
        k("Shift+o"); k("k");
        check(name.get() == "Ok", &format!("typing reaches the field: {:?}", name.get()))?;
        k("Tab");
        check(focus::is_focused(&*save), "Tab moves to Save")?;
        win.snapshot(shots.join("kb-1-button.png")).map_err(|e| e.to_string())?;
        k("Enter"); k("Space");
        check(clicks.get() == 2, "Enter and Space press the button")?;
        k("Tab");
        check(focus::is_focused(&*group.button(0).unwrap()), "Tab enters the button group at the selected button")?;
        k("Right"); k("Right"); k("Right");
        check(view.get() == 2 && focus::is_focused(&*group.button(2).unwrap()), "Right moves selection and focus, stops at the end")?;
        k("Tab");
        check(focus::is_focused(&*tabs), "button group is one Tab stop")?;
        k("Right"); k("End");
        check(tab.get() == 2, "Right/End pick tabs")?;
        k("Ctrl+Tab");
        check(tab.get() == 0, "Ctrl+Tab wraps to the first tab")?;
        k("Ctrl+Shift+Tab");
        check(tab.get() == 2, "Ctrl+Shift+Tab goes back")?;
        win.snapshot(shots.join("kb-2-tabs.png")).map_err(|e| e.to_string())?;
        k("Tab");
        check(focus::is_focused(&*list), "Tab reaches the list")?;
        k("Down"); k("Down"); k("j"); k("k");
        check(sel.get() == Some(1), &format!("arrows and j/k move the cursor row: {:?}", sel.get()))?;
        k("Enter");
        check(*opened.borrow() == Some(1), "Enter opens the row")?;
        win.snapshot(shots.join("kb-3-list.png")).map_err(|e| e.to_string())?;
        k("Tab");
        check(focus::current().is_some_and(|f| f.as_widget_ptr() == field.input().as_widget_ptr()), "Tab wraps to the first control")?;
        k("Shift+Tab");
        check(focus::is_focused(&*list), "Shift+Tab goes back")?;
        k("Up");
        check(focus::is_focused(&*list) && sel.get() == Some(0), "arrows never leave the focused control")?;

        // Menus and dialogs take focus and give it back.
        let mut s2 = save.clone();
        let _ = s2.take_focus();
        let picked = Rc::new(RefCell::new(None));
        let p2 = picked.clone();
        PopupMenu::open(&*save, vec![PopupItem::new("", "Copy"), PopupItem::new("", "Cut").disabled(), PopupItem::new("", "Paste")], move |i| *p2.borrow_mut() = Some(i));
        check(!focus::is_focused(&*save), "the menu has the keyboard")?;
        k("Tab");
        check(!focus::is_focused(&*save), "Tab does not leave an open menu")?;
        win.snapshot(shots.join("kb-4-menu.png")).map_err(|e| e.to_string())?;
        k("Down"); k("Enter");
        check(*picked.borrow() == Some(2), &format!("Down skips the disabled entry: {:?}", picked.borrow()))?;
        check(focus::is_focused(&*save), "focus is back on the menu button")?;
        let answer = Rc::new(RefCell::new(None));
        let a2 = answer.clone();
        ConfirmDialog::open(&*save, "Delete 3 files?", "Delete", true, move |ok| *a2.borrow_mut() = Some(ok));
        k("Escape");
        check(*answer.borrow() == Some(false) && focus::is_focused(&*save), "Esc cancels the dialog and focus comes back")?;

        // Notifications and the operation toast are reachable with Tab.
        let mut l2 = list.clone();
        let _ = l2.take_focus();
        let cancelled = Rc::new(RefCell::new(false));
        let c3 = cancelled.clone();
        let mut toast = OperationToast::show(&*win, "Copying 3 files").ok_or("toast")?.on_cancel(move || *c3.borrow_mut() = true);
        toast.set_progress(0.4);
        k("Tab");
        check(toast.has_focus(), "Tab reaches the operation toast")?;
        k("Space");
        check(toast.is_paused(), "Space pauses")?;
        k("Right");
        win.snapshot(shots.join("kb-5-toast.png")).map_err(|e| e.to_string())?;
        k("Enter");
        check(*cancelled.borrow(), "Right + Enter cancels")?;
        k("Escape");
        check(focus::is_focused(&*list), "Esc gives focus back to the list")?;
        toast.close();
        let before = Notification::count();
        Notification::show(&*win, "", "Saved", "3 files", Urgency::Critical);
        k("Tab");
        k("Enter");
        check(Notification::count() == before && focus::is_focused(&*list), "Enter dismisses a focused notification; focus comes back")
    });
    run("live theme switch: omarchy-theme-set style rename is picked up", &|shots| {
        let state = PathBuf::from(std::env::var("OMARCHY_FLTK_STATE_DIR").unwrap());
        std::fs::create_dir_all(state.join("next-theme")).unwrap();
        std::fs::write(state.join("next-theme/colors.toml"), COLORS_B).unwrap();
        std::fs::remove_dir_all(state.join("theme")).unwrap();
        std::fs::rename(state.join("next-theme"), state.join("theme")).unwrap();
        pump_until(|| theme::current().palette.accent == theme::Rgb(0x33ccff));
        check(theme::current().palette.accent == theme::Rgb(0x33ccff), "accent after swap")?;
        let mut win = AppWindow::new("t", 300, 140);
        let mut body = win.body();
        let b = Button::new("Go").primary();
        body.fit(&*b);
        win.end();
        let p = shots.join("button-theme-b.png");
        win.snapshot(&p).map_err(|e| e.to_string())?;
        let px = pixel(&p, b.x() + 3, b.y() + 3)?;
        check(px == [0x33, 0xcc, 0xff], &format!("pixel {px:?}"))?;
        // Edit in place after the swap: the watcher must follow the new dir.
        std::fs::write(state.join("theme/colors.toml"), COLORS_A).unwrap();
        pump_until(|| theme::current().palette.accent == theme::Rgb(0xe68e0d));
        check(theme::current().palette.accent == theme::Rgb(0xe68e0d), "accent after in-place edit")
    });
    run("theme: partial writes keep the last valid theme, recovery and overrides apply", &|_| {
        let state = PathBuf::from(std::env::var("OMARCHY_FLTK_STATE_DIR").unwrap());
        let user = PathBuf::from(std::env::var("OMARCHY_FLTK_USER_SHELL").unwrap());
        let colors = state.join("theme/colors.toml");
        std::fs::write(&colors, COLORS_A).unwrap();
        theme::reload();
        let accent = |c: u32| theme::current().palette.accent == theme::Rgb(c);
        check(accent(0xe68e0d), "starts on theme A")?;
        // Cut in the middle of a write: the watcher sees it, nothing changes.
        std::fs::write(&colors, "accent = \"#33ccff\"\nbackground = \"#1a1b").unwrap();
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_millis(400) { App::pump(); std::thread::sleep(Duration::from_millis(10)); }
        check(accent(0xe68e0d), "truncated colors.toml keeps the last theme")?;
        check(!theme::reload() && matches!(theme::last_error(), Some(theme::ThemeError::Parse { line: 2, .. })), &format!("parse error reported: {:?}", theme::last_error()))?;
        std::fs::write(&colors, "").unwrap();
        check(!theme::reload() && matches!(theme::last_error(), Some(theme::ThemeError::Missing { .. })), "empty file: missing keys")?;
        std::fs::write(&colors, "accent = \"#33ccff\"\nbackground = \"#1a1b2\"\nforeground = \"#c0caf5\"\n").unwrap();
        check(!theme::reload() && matches!(theme::last_error(), Some(theme::ThemeError::BadColor { .. })), "cut color value")?;
        check(accent(0xe68e0d), "still theme A")?;
        // Directory being replaced, then the new theme lands.
        std::fs::remove_dir_all(state.join("theme")).unwrap();
        check(!theme::reload() && matches!(theme::last_error(), Some(theme::ThemeError::Read { .. })), "missing theme dir")?;
        std::fs::create_dir_all(state.join("next")).unwrap();
        std::fs::write(state.join("next/colors.toml"), COLORS_B).unwrap();
        std::fs::rename(state.join("next"), state.join("theme")).unwrap();
        pump_until(|| accent(0x33ccff));
        check(accent(0x33ccff) && theme::last_error().is_none(), "recovers with the next valid theme")?;
        // User shell.toml overrides: valid applies, malformed keeps, removal reverts.
        std::fs::write(&user, "[font]\nbase-size = 14\n").unwrap();
        theme::reload();
        check(theme::current().font.base == 14, "user override applied")?;
        std::fs::write(&user, "[font\nbase-size = 18\n").unwrap();
        check(!theme::reload() && theme::current().font.base == 14, "malformed override keeps the last theme")?;
        std::fs::remove_file(&user).unwrap();
        theme::reload();
        check(theme::current().font.base == 12, "override removed")?;
        std::fs::write(&colors_path(&state), COLORS_A).unwrap();
        theme::reload();
        // Issue #8: extended colors switch with the theme, all from the same
        // validated load, and a malformed reload keeps the last valid set.
        let ext = |bg: &str, d: &str, l: &str, b: &str| format!(
            "accent = \"#e68e0d\"\nbackground = \"{bg}\"\nforeground = \"#bebebe\"\ndarker_background = \"{d}\"\nlighter_background = \"{l}\"\nbright_foreground = \"{b}\"\n");
        let set = || { let p = theme::current().palette; (p.background, p.darker_background, p.lighter_background, p.bright_foreground) };
        let seen = Rc::new(RefCell::new(Vec::new()));
        let s2 = seen.clone();
        theme::on_change(move |t| { let p = &t.palette; s2.borrow_mut().push((p.background, p.darker_background, p.lighter_background, p.bright_foreground)); });
        let x = (theme::Rgb(0x101010), theme::Rgb(0x050505), theme::Rgb(0x202020), theme::Rgb(0xf0f0f0));
        let y = (theme::Rgb(0xfafafa), theme::Rgb(0xdedede), theme::Rgb(0xf5f5f5), theme::Rgb(0x000000));
        std::fs::write(colors_path(&state), ext("#101010", "#050505", "#202020", "#f0f0f0")).unwrap();
        check(theme::reload() && set() == x, &format!("theme X: {:?}", set()))?;
        std::fs::write(colors_path(&state), ext("#fafafa", "#dedede", "#f5f5f5", "#000000")).unwrap();
        check(theme::reload() && set() == y, &format!("theme Y: {:?}", set()))?;
        // Malformed: new values for all three, one of them cut mid-write.
        std::fs::write(colors_path(&state), ext("#101010", "#050505", "#2020", "#f0f0f0")).unwrap();
        check(!theme::reload() && matches!(theme::last_error(), Some(theme::ThemeError::BadColor { ref key, .. }) if key == "lighter_background"),
              &format!("cut lighter_background is rejected: {:?}", theme::last_error()))?;
        check(set() == y, &format!("all three stay on theme Y, with its background: {:?}", set()))?;
        // Malformed TOML (file cut after darker_background).
        std::fs::write(colors_path(&state), "background = \"#101010\"\nforeground = \"#bebebe\"\ndarker_background = \"#0505").unwrap();
        check(!theme::reload() && set() == y, "truncated file: still theme Y")?;
        // Absent keys: documented defaults, still one coherent theme.
        std::fs::write(colors_path(&state), "background = \"#121212\"\nforeground = \"#bebebe\"\n").unwrap();
        check(theme::reload() && set() == (theme::Rgb(0x121212), theme::Rgb(0x0a0a0a), theme::Rgb(0x252525), theme::Rgb(0xbebebe)), &format!("defaults: {:?}", set()))?;
        check(*seen.borrow() == vec![x, y, (theme::Rgb(0x121212), theme::Rgb(0x0a0a0a), theme::Rgb(0x252525), theme::Rgb(0xbebebe))],
              &format!("listeners saw exactly the valid themes, whole: {:?}", seen.borrow()))?;
        std::fs::write(colors_path(&state), COLORS_A).unwrap();
        theme::reload();
        // No Omarchy at all: the built-in fallback still loads.
        std::env::set_var("OMARCHY_FLTK_STATE_DIR", state.join("does-not-exist"));
        let fallback = theme::Theme::try_load();
        std::env::set_var("OMARCHY_FLTK_STATE_DIR", &state);
        check(fallback.is_ok_and(|t| t.palette.accent == theme::Rgb(0x7aa2f7)), "fallback without Omarchy")
    });
    drop(app);
    println!("{}", if failed == 0 { "all ui checks passed" } else { "ui checks FAILED" });
    std::process::exit(if failed == 0 { 0 } else { 1 });
}

fn colors_path(state: &Path) -> PathBuf {
    state.join("theme/colors.toml")
}

fn check(ok: bool, what: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(what.to_string()) }
}

fn pump_until(mut f: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(3);
    while !f() && Instant::now() < end {
        App::pump();
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Reads one RGB pixel from a PNG written by our encoder (stored deflate).
fn pixel(path: &Path, x: i32, y: i32) -> Result<[u8; 3], String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let w = u32::from_be_bytes(data[16..20].try_into().unwrap()) as usize;
    let depth = if data[25] == 6 { 4 } else { 3 };
    let (mut pos, mut z) = (8, Vec::new());
    while pos + 8 <= data.len() {
        let len = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        if &data[pos + 4..pos + 8] == b"IDAT" { z.extend_from_slice(&data[pos + 8..pos + 8 + len]); }
        pos += 12 + len;
    }
    let (mut raw, mut i) = (Vec::new(), 2);
    loop {
        let last = z[i] & 1;
        let len = u16::from_le_bytes([z[i + 1], z[i + 2]]) as usize;
        raw.extend_from_slice(&z[i + 5..i + 5 + len]);
        i += 5 + len;
        if last == 1 { break; }
    }
    let stride = w * depth + 1;
    let off = y as usize * stride + 1 + x as usize * depth;
    Ok([raw[off], raw[off + 1], raw[off + 2]])
}
