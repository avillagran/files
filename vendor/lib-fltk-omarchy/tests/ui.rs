//! UI tests on the main thread (fltk is single-threaded), offscreen: no
//! window is shown. Each check renders with `snapshot` and reads pixels.
//! Uses a fake Omarchy state dir so results do not depend on the desktop.
//!
//! Per OS (docs/PLATFORMS.md): on Linux the checks need a display (Wayland
//! or X11, e.g. `xvfb-run`) and are skipped without one. The pixel
//! expectations were calibrated on Linux; on macOS and Windows failures
//! are reported but do not fail the run unless `OMARCHY_FLTK_UI_STRICT=1`.
use fltk::prelude::*;
use fltk_omarchy::focus::{self, press};
use fltk_omarchy::gfx::{self, Hit, View};
use fltk_omarchy::layout::{self, FlexExt};
use fltk_omarchy::list::{List, ListRow};
use fltk_omarchy::model::ListModel;
use fltk_omarchy::panel::{KeyboardPanel, PanelLayout};
use fltk_omarchy::overlay::{ConfirmDialog, Dropdown, Menu, MenuItem, Notification, Urgency};
use fltk_omarchy::popups::{Dismiss, OperationToast, PopupCard, PopupItem, PopupMenu, SearchableDropdown, ShortcutHelp};
use fltk_omarchy::table::{Column, Table, Tree, TreeNode};
use fltk_omarchy::text::{Block, ColorRole, RichLabel, RichText, Style, TextPos};
use fltk_omarchy::theme::Corners;
use fltk_omarchy::Mode;
use fltk_omarchy::task::{Outcome, Task};
use fltk_omarchy::widgets::*;
use fltk_omarchy::{i18n, theme, tr, trn, App, AppWindow, State};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

mod edit_ui;
mod tooltip_ui;
mod motion_ui;

const COLORS_A: &str = "accent = \"#e68e0d\"\nbackground = \"#121212\"\nforeground = \"#bebebe\"\nred = \"#d35f5f\"\n";
const COLORS_B: &str = "accent = \"#33ccff\"\nbackground = \"#1a1b26\"\nforeground = \"#c0caf5\"\nred = \"#f7768e\"\n";

fn main() {
    use fltk_omarchy::platform::{Os, OS};
    let has = |v: &str| std::env::var_os(v).is_some_and(|x| !x.is_empty());
    if OS == Os::Linux && !has("WAYLAND_DISPLAY") && !has("DISPLAY") {
        println!("ui checks skipped: no display (set WAYLAND_DISPLAY or DISPLAY, e.g. xvfb-run)");
        return;
    }
    let strict = OS == Os::Linux || std::env::var("OMARCHY_FLTK_UI_STRICT").is_ok_and(|v| v == "1");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("build/test-ui");
    let _ = std::fs::remove_dir_all(&root);
    let state = root.join("state");
    std::fs::create_dir_all(state.join("theme")).unwrap();
    std::fs::write(state.join("theme/colors.toml"), COLORS_A).unwrap();
    std::fs::write(state.join("theme.name"), "A").unwrap();
    std::env::set_var("OMARCHY_FLTK_STATE_DIR", &state);
    std::env::set_var("OMARCHY_FLTK_USER_SHELL", root.join("no-user-shell.toml"));
    std::env::set_var("OMARCHY_FLTK_TOOLTIPS", root.join("tooltips.json"));
    // Hyprland values are read live; pin them by hiding the socket.
    std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");

    let app = App::new("dev.omarchy.fltk.tests");
    // Final states unless a check turns animations on (AGENTS.md rule 11).
    fltk_omarchy::anim::set_enabled(false);
    // Toolkit strings follow the system language: pin English.
    i18n::set_locale("en");
    let shots = root.join("shots");
    let mut failed = 0;
    // OMARCHY_FLTK_UI_ONLY=<a|b>: run only the checks whose name contains one of them.
    let only = std::env::var("OMARCHY_FLTK_UI_ONLY").ok();
    let mut run = |name: &str, f: &dyn Fn(&Path) -> Result<(), String>| {
        if only.as_deref().is_some_and(|o| !o.split('|').any(|p| name.contains(p))) {
            return;
        }
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
        // Within 2 per channel: Windows' GDI image blit rounds premultiplied colors.
        let near = |a: [u8; 3], b: [u8; 3]| a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 2);
        check(near(mid, [acc.r(), acc.g(), acc.b()]), &format!("card center {mid:?}"))?;
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
        check(px.iter().zip([0x33u8, 0xcc, 0xff]).all(|(a, b)| a.abs_diff(b) <= 2), &format!("theme color reaches the view: {px:?}"))
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
    run("data models: rows redraw by item, scroll anchored on insert above, selection by key, feeds coalesce", &|shots| {
        #[derive(Clone, PartialEq)]
        struct Th { id: u32, title: String, status: &'static str }
        let th = |id: u32| Th { id, title: format!("Thread {id}"), status: "idle" };
        let model = ListModel::with_items(|t: &Th| t.id, (0..1000).map(th));
        let mut win = AppWindow::new("t", 400, 400);
        let mut body = win.body();
        let sel = State::new(None::<usize>);
        let mut list = List::from_model(&model, |t| ListRow::new(&t.title).detail(t.status)).bind_selected(&sel);
        body.fit(&*list);
        win.end();
        let rh = theme::with(|t| t.font.body + 4 + 2 * t.spacing.xl) as i64;
        list.set_scroll_offset(rh * 100 + 5);
        win.snapshot(shots.join("model-list-0.png")).map_err(|e| e.to_string())?;
        let built = list.rows_built();
        let inv0 = list.invalidations();
        model.patch(&102, |t| t.status = "running");
        let inv1 = list.invalidations();
        check(inv1.rows == inv0.rows + 1 && inv1.full == inv0.full, &format!("a visible patch invalidates one row: {inv0:?} -> {inv1:?}"))?;
        model.patch(&900, |t| t.status = "running");
        model.patch(&900, |t| t.status = "running"); // equal: no notification at all
        let inv2 = list.invalidations();
        check(inv2.rows == inv1.rows && inv2.none == inv1.none + 1 && inv2.full == inv1.full, &format!("an offscreen patch redraws nothing: {inv2:?}"))?;
        // Selection by key and scroll anchoring on insert above the viewport.
        list.select(103);
        let sid = list.selected_id();
        let p0 = shots.join("model-list-1-before.png");
        win.snapshot(&p0).map_err(|e| e.to_string())?;
        let top = list.scroll_offset();
        model.transaction(|m| for k in 0..5 { m.insert(0, th(5000 + k)); });
        let inv3 = list.invalidations();
        check(list.scroll_offset() == top + 5 * rh, &format!("anchored: {} -> {} (row {rh})", top, list.scroll_offset()))?;
        check(list.selected() == Some(108) && sel.get() == Some(108) && list.selected_id() == sid, &format!("selection follows its item: {:?} {:?}", list.selected(), sel.get()))?;
        check(inv3.rows == inv2.rows && inv3.full == inv2.full, &format!("insert above: no row redrawn: {inv3:?}"))?;
        let p1 = shots.join("model-list-2-after.png");
        win.snapshot(&p1).map_err(|e| e.to_string())?;
        let (a, b) = (png_rgb(&p0)?, png_rgb(&p1)?);
        let same = (list.y()..list.y() + list.h()).all(|y| (list.x()..list.x() + list.w() - 8).all(|x| a.at(x, y) == b.at(x, y)));
        check(same, "the rows on screen are pixel-identical after inserting above")?;
        // Moving the selected item: selection by key, the view keeps its offset.
        model.move_key(&103, 999);
        check(list.selected() == Some(999) && list.scroll_offset() == top + 5 * rh, &format!("moved item keeps the selection: {:?}", list.selected()))?;
        check(list.rows_built() - built < 120, "virtual: only visible rows were built")?;

        // A thread pushes 500 diffs; the UI applies them in one notification.
        let feed = model.feed();
        let n0 = model.notifications();
        let t0 = Instant::now();
        std::thread::spawn(move || {
            for i in 0..500u32 { feed.patch(i, |t| t.status = "done"); }
            feed.upsert(Th { id: 9999, title: "New".into(), status: "idle" });
        }).join().map_err(|_| "feed thread")?;
        pump_until(|| model.contains_key(&9999));
        check(model.notifications() - n0 == 1, &format!("one notification for 501 diffs: {}", model.notifications() - n0))?;
        check(model.with_key(&499, |t| t.status) == Some("done") && t0.elapsed() < Duration::from_secs(2), "diffs applied")?;
        let text = State::new(String::new());
        let edits = Rc::new(RefCell::new(Vec::new()));
        let e2 = edits.clone();
        let _sub = text.subscribe_edit(move |t, e| e2.borrow_mut().push((t.len(), e)));
        let tf = text.text_feed();
        std::thread::spawn(move || for _ in 0..1000 { tf.push_str("token ") }).join().map_err(|_| "text thread")?;
        pump_until(|| text.with(|t| t.len()) == 6000);
        check(*edits.borrow() == vec![(6000, fltk_omarchy::state::Edit::Appended { from: 0 })], &format!("1000 chunks, one append: {:?}", edits.borrow()))?;
        let level = State::new(0u32);
        let hits = Rc::new(Cell::new(0));
        let h2 = hits.clone();
        let _lsub = level.subscribe(move |_| h2.set(h2.get() + 1));
        let lf = level.feed().max_rate(20.0);
        let started = std::time::Instant::now();
        let producer = std::thread::spawn(move || for i in 1..=60 { lf.set(i); std::thread::sleep(Duration::from_millis(5)); });
        pump_until(|| level.get() == 60);
        producer.join().map_err(|_| "level thread")?;
        // At most one notification per 50 ms of wall time (slow CI runners sleep longer than 5 ms).
        let allowed = (started.elapsed().as_secs_f64() * 20.0).ceil() as u32 + 1;
        check(level.get() == 60 && hits.get() <= allowed.max(10) && hits.get() < 60, &format!("20 Hz cap: 60 values in {} notifications (allowed {allowed})", hits.get()))?;

        // Table and Tree on the same kind of model.
        let panel = KeyboardPanel::new("threads");
        let mut w2 = AppWindow::new("t", 520, 360);
        let mut b2 = w2.body();
        let table = Table::from_model(&panel, &model, vec![Column::new("Title", 0).flex(), Column::new("Status", 90)],
            |t, col| if col == 0 { t.title.clone() } else { t.status.to_string() });
        b2.fit(&*table);
        w2.end();
        let trh = theme::with(|t| t.font.body + 4 + 2 * t.spacing.lg) as i64;
        table.set_scroll_offset(trh * 50);
        w2.snapshot(shots.join("model-table.png")).map_err(|e| e.to_string())?;
        panel.select_only(model.id_of(&55).unwrap());
        let (ti0, ttop) = (table.invalidations(), table.scroll_offset());
        model.patch(&50, |t| t.status = "running"); // 50 is at index 55 (5 inserted at the top)
        let ti1 = table.invalidations();
        check(ti1.rows == ti0.rows + 1 && ti1.full == ti0.full, &format!("table: one row for a visible patch: {ti0:?} -> {ti1:?}"))?;
        model.transaction(|m| { m.insert(0, th(7000)); m.insert(0, th(7001)); });
        check(table.scroll_offset() == ttop + 2 * trh && panel.cursor() == model.id_of(&55), &format!("table anchored, cursor by ID: {} -> {}", ttop, table.scroll_offset()))?;

        #[derive(Clone, PartialEq)]
        struct Node { id: u32, parent: Option<u32>, label: String }
        let nodes = ListModel::with_items(|n: &Node| n.id, (0..3).map(|i| Node { id: i, parent: None, label: format!("Project {i}") })
            .chain((0..30).map(|j| Node { id: 100 + j, parent: Some(j % 3), label: format!("Thread {j}") })));
        let tp = KeyboardPanel::new("tree");
        let mut w3 = AppWindow::new("t", 360, 320);
        let mut b3 = w3.body();
        let tree = Tree::from_model(&tp, &nodes, |n| { let t = TreeNode::new(0, &n.label); if n.parent.is_none() { t.branch() } else { t } }, |n| n.parent);
        b3.fit(&*tree);
        w3.end();
        let root0 = nodes.id_of(&0).unwrap();
        tree.expand(root0);
        check(tree.visible_len() == 13, &format!("roots + 10 children of project 0: {}", tree.visible_len()))?;
        w3.snapshot(shots.join("model-tree.png")).map_err(|e| e.to_string())?;
        let tv0 = tree.invalidations();
        nodes.patch(&103, |n| n.label = "Renamed".into());
        let tv1 = tree.invalidations();
        check(tv1.rows == tv0.rows + 1 && tv1.full == tv0.full && tree.node(2).is_some_and(|n| n.label == "Renamed"), &format!("tree: one row for a rename: {tv0:?} -> {tv1:?}"))?;
        tp.select_only(nodes.id_of(&109).unwrap());
        let row109 = tp.cursor_index();
        nodes.patch(&106, |n| n.parent = Some(1)); // reparent above the cursor: rebuild, selection by ID
        check(tree.visible_len() == 12 && tp.cursor() == nodes.id_of(&109) && tp.cursor_index() == row109.map(|r| r - 1),
            &format!("tree: reparenting rebuilds and keeps the selection: {} {:?}", tree.visible_len(), tp.cursor_index()))?;
        nodes.push(Node { id: 500, parent: Some(0), label: "New thread".into() });
        check(tree.visible_len() == 13 && tree.node(10).is_some_and(|n| n.label == "New thread"), "tree: a new child shows under its expanded parent")
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
        std::env::set_var("OMARCHY_FLTK_APPEARANCE", "dark"); // independent of the OS setting
        let fallback = theme::Theme::try_load();
        std::env::remove_var("OMARCHY_FLTK_APPEARANCE");
        std::env::set_var("OMARCHY_FLTK_STATE_DIR", &state);
        check(fallback.is_ok_and(|t| t.palette.accent == theme::Rgb(0x7aa2f7)), "fallback without Omarchy")
    });
    run("platform: Mod+, shortcut, built-in light/dark theme, theme dir override", &|_| {
        let mut win = AppWindow::new("t", 300, 140);
        let mut body = win.body();
        let mut b = Button::new("Focus");
        body.fit(&*b);
        win.end();
        win.snapshot(std::env::temp_dir().join("omarchy-fltk-unused.png")).ok();
        let hits = Rc::new(Cell::new(0));
        let h = hits.clone();
        win.shortcut("Mod+,", move || h.set(h.get() + 1));
        let _ = b.take_focus();
        press("Mod+,");
        check(hits.get() == 1, &format!("Mod+, runs once: {}", hits.get()))?;
        let other = if fltk_omarchy::keys::PRIMARY == fltk::enums::Shortcut::Ctrl { "Super+," } else { "Ctrl+," };
        press(other);
        check(hits.get() == 1, &format!("{other} is not Mod+,"))?;
        check(fltk_omarchy::keys::label("Mod+,") == if OS == Os::MacOs { "⌘," } else { "Ctrl+," }, "label")?;
        // No theme files: the built-in theme follows the appearance.
        let state = theme::omarchy_state_dir();
        std::env::set_var("OMARCHY_FLTK_STATE_DIR", state.join("does-not-exist"));
        let pick = |a: &str| { std::env::set_var("OMARCHY_FLTK_APPEARANCE", a); theme::Theme::try_load().map(|t| t.palette.accent) };
        let (light, dark) = (pick("light"), pick("dark"));
        std::env::remove_var("OMARCHY_FLTK_APPEARANCE");
        std::env::set_var("OMARCHY_FLTK_STATE_DIR", &state);
        check(light == Ok(theme::Rgb(0x2e7de9)) && dark == Ok(theme::Rgb(0x7aa2f7)), &format!("built-in light/dark: {light:?} {dark:?}"))?;
        // Any directory with Omarchy theme files, on any OS.
        let dir = state.parent().unwrap().join("custom-theme");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("colors.toml"), COLORS_B).unwrap();
        check(theme::set_theme_dir(Some(dir.clone())) && theme::current().palette.accent == theme::Rgb(0x33ccff), "set_theme_dir applies it")?;
        check(theme::theme_dir() == dir, "theme_dir reports it")?;
        // The watcher follows the new directory (inotify on Linux, polling elsewhere).
        std::fs::write(dir.join("colors.toml"), COLORS_A.replace("#e68e0d", "#123456")).unwrap();
        pump_until(|| theme::current().palette.accent == theme::Rgb(0x123456));
        check(theme::current().palette.accent == theme::Rgb(0x123456), "an edit in the new directory is followed live")?;
        check(theme::set_theme_dir(None) && theme::current().palette.accent == theme::Rgb(0xe68e0d), "None goes back to the Omarchy theme")
    });
    run("i18n: switching the locale re-labels bound widgets live", &|shots| {
        i18n::set_locale("en");
        let count = State::new(3);
        let mut win = AppWindow::new("Files", 420, 330);
        win.bind_title(&i18n::text("Preferences"));
        let mut body = win.body();
        let mut row = layout::row();
        let open = Button::new("").bordered().bind_text(&i18n::text("Open")).bind_tooltip(&i18n::text("Open"));
        row.fit(&*open);
        let trash = Button::new("").bind_text(&i18n::text("Move to trash"));
        row.fit(&*trash);
        row.close_fitted();
        // Temporaries: the binding keeps the derived States alive.
        let items = Label::new("").bind(&i18n::map(&count, |n| trn!("{count} items", *n)));
        body.fit(&*items);
        let plain = Label::new("").bind(&count.map(|n| format!("n = {n}")));
        body.fit(&*plain);
        let field = TextField::new().bind_placeholder(&i18n::text("Search…"));
        body.fit(&*field);
        let toggle = Toggle::new("", "", &State::new(true)).bind_title(&i18n::text("Fullscreen"));
        body.fit(&*toggle);
        win.end();
        win.snapshot(shots.join("i18n-en.png")).map_err(|e| e.to_string())?;
        check(open.text() == "Open" && items.text() == "3 items" && win.label() == "Preferences", "english")?;
        let w_en = trash.w();
        i18n::set_locale("de_DE.UTF-8");
        check(open.text() == "Öffnen" && trash.text() == "In den Papierkorb verschieben", &format!("german buttons: {} / {}", open.text(), trash.text()))?;
        check(WidgetExt::tooltip(&*open).as_deref() == Some("Öffnen"), "tooltip follows")?;
        check(win.label() == "Einstellungen" && win.header().title() == "Einstellungen", "window and header title")?;
        check(trash.w() > w_en && trash.w() == trash.preferred_size().0, &format!("fitted button takes its translated width: {} -> {}", w_en, trash.w()))?;
        check(items.text() == "3 Elemente", &items.text())?;
        win.snapshot(shots.join("i18n-de.png")).map_err(|e| e.to_string())?;
        count.set(1);
        check(items.text() == "1 Element" && plain.text() == "n = 1", "plural and derived temporary follow the count")?;
        i18n::set_locale("ru");
        let forms: Vec<String> = [1, 3, 5, 21].iter().map(|n| { count.set(*n); items.text() }).collect();
        check(forms == ["1 элемент", "3 элемента", "5 элементов", "21 элемент"], &format!("{forms:?}"))?;
        check(tr!("No matches for “{query}”", query = "x") == "Нет совпадений для «x»", "toolkit string with an argument")?;
        win.snapshot(shots.join("i18n-ru.png")).map_err(|e| e.to_string())?;
        i18n::set_locale("en");
        check(open.text() == "Open" && items.text() == "21 items", "back to English")
    });
    run("i18n: Arabic and CJK render; start-aligned labels follow RTL", &|shots| {
        for loc in ["ar", "ja", "zh", "ko"] {
            i18n::set_locale(loc);
            let mut win = AppWindow::new("", 460, 200);
            win.bind_title(&i18n::text("Keyboard shortcuts"));
            let mut body = win.body();
            let label = Label::new("").bind(&i18n::text("Nothing here yet"));
            body.fit(&*label);
            let n = State::new(5);
            body.fit(&*Label::new("").tone(Tone::Dim).bind(&i18n::map(&n, |n| trn!("{count} items", *n))));
            let mut row = layout::row();
            row.fit(&*Button::new("").bordered().bind_text(&i18n::text("Cancel")));
            row.fit(&*Button::new("").primary().bind_text(&i18n::text("Save")));
            row.close_fitted();
            win.end();
            let p = shots.join(format!("i18n-{loc}.png"));
            win.snapshot(&p).map_err(|e| e.to_string())?;
            // Text was drawn: foreground pixels in the label's start (or end, RTL) half.
            let bg = pixel(&p, 2, win.h() - 3)?;
            let (lx, ly, lw, lh) = (label.x(), label.y(), label.w(), label.h());
            let half = |right: bool| -> Result<usize, String> {
                let mut n = 0;
                for y in (ly..ly + lh).step_by(2) {
                    for x in (lx..lx + lw / 2).step_by(2) {
                        let x = if right { x + lw / 2 } else { x };
                        if pixel(&p, x, y)? != bg { n += 1; }
                    }
                }
                Ok(n)
            };
            let (l, r) = (half(false)?, half(true)?);
            let rtl = i18n::is_rtl();
            check(rtl == (loc == "ar"), "direction")?;
            if rtl { check(r > 0 && l == 0, &format!("{loc}: RTL label is right-aligned ({l} left, {r} right)"))?; }
            else { check(l > 0, &format!("{loc}: text drawn ({l} px)"))?; }
        }
        i18n::set_locale("en");
        check(i18n::direction() == i18n::Direction::Ltr, "back to LTR")
    });
    run("i18n: toolkit overlays draw their own strings translated", &|shots| {
        i18n::set_locale("uk");
        let mut win = AppWindow::new("t", 460, 300);
        win.end();
        ConfirmDialog::open(&*win, &tr!("Delete"), &tr!("Delete"), true, |_| {});
        win.snapshot(shots.join("i18n-uk-confirm.png")).map_err(|e| e.to_string())?;
        focus::press("Escape");
        let toast = OperationToast::show(&*win, "Копіювання").ok_or("toast")?;
        let mut t2 = toast.clone();
        t2.toggle_pause();
        win.snapshot(shots.join("i18n-uk-toast.png")).map_err(|e| e.to_string())?;
        toast.close();
        Menu::open(&*win, "Меню", vec![], |_| {});
        win.snapshot(shots.join("i18n-uk-menu.png")).map_err(|e| e.to_string())?;
        focus::press("Escape");
        i18n::set_locale("en");
        check(tr!("Cancel") == "Cancel", "back to English")
    });
    run("rtl: header, rows, toggles mirror live (no rebuild); direction(Ltr) override; plain Flex rows", &|shots| {
        i18n::set_locale("en");
        let mut win = AppWindow::new("Title", 460, 320);
        let act = win.header().action(glyph::SETTINGS, "Preferences", || {});
        let mut body = win.body();
        let (a, b, c) = (Button::new("A").bordered(), Button::new("B").bordered(), Button::new("C").bordered());
        let mut row = layout::row();
        row.fit(&*a);
        row.fit(&*b);
        row.fit(&*c);
        row.close_fitted();
        let (x, y) = (Button::new("X").bordered(), Button::new("Y").bordered());
        let mut media = layout::row();
        media.direction(layout::Direction::Ltr);
        media.fit(&*x);
        media.fit(&*y);
        media.close_fitted();
        // A row made with plain fltk-rs (AppWindow::end registers it).
        let plain = fltk::group::Flex::default().row();
        let p1 = fltk::frame::Frame::default();
        let p2 = fltk::frame::Frame::default();
        plain.end();
        body.fixed(&plain, 20);
        let toggle = Toggle::new("Night light", "", &State::new(true));
        body.fit(&*toggle);
        win.end();
        let en = shots.join("rtl-live-en.png");
        win.snapshot(&en).map_err(|e| e.to_string())?;
        check(a.x() < b.x() && b.x() < c.x() && x.x() < y.x() && p1.x() < p2.x(), "en: left to right")?;
        check(act.x() > win.w() / 2, "en: actions on the right")?;
        let (ax, cx) = (a.x(), c.x() + c.w());
        i18n::set_locale("ar");
        let ar = shots.join("rtl-live-ar.png");
        win.snapshot(&ar).map_err(|e| e.to_string())?;
        check(a.x() > b.x() && b.x() > c.x(), &format!("ar: row mirrored ({} {} {})", a.x(), b.x(), c.x()))?;
        check(win.w() - (a.x() + a.w()) == ax, &format!("ar: first child at the right edge ({} vs {ax})", win.w() - a.x() - a.w()))?;
        check(c.x() == win.w() - cx, "ar: last child where the mirror of its end was")?;
        check(x.x() < y.x(), "ar: direction(Ltr) row keeps its order")?;
        check(p1.x() > p2.x(), "ar: plain Flex row mirrored")?;
        check(act.x() < win.w() / 2, &format!("ar: header actions on the left ({})", act.x()))?;
        // Header title on the right half: ink in the right half of the header, none in the middle-left.
        let bg = pixel(&ar, win.w() / 2, 4)?;
        let hh = win.header().h();
        let mut right_ink = 0;
        let mut left_ink = 0;
        for yy in (4..hh - 4).step_by(2) {
            for xx in (win.w() / 2..win.w() - 4).step_by(2) {
                if pixel(&ar, xx, yy)? != bg { right_ink += 1; }
            }
            for xx in (act.x() + act.w() + 4..win.w() / 2).step_by(2) {
                if pixel(&ar, xx, yy)? != bg { left_ink += 1; }
            }
        }
        check(right_ink > 10 && left_ink == 0, &format!("ar: header title on the right ({right_ink} right, {left_ink} left)"))?;
        // Toggle on: the knob is on the left end of the switch, the mirror of en.
        let t = theme::current();
        let th = t.spacing.control_height.max(28);
        let track_h = 22.max((th as f64 * 0.55).round() as i32);
        let track_w = (track_h as f64 * 1.9).round() as i32;
        let knob = 6.max((track_h as f64 * 0.72).round() as i32);
        let inset = 1.max((track_h - knob) / 2);
        let pad = t.spacing.row_padding_x + 1;
        let cy = toggle.y() + toggle.h() / 2;
        let knob_en = pixel(&en, toggle.x() + toggle.w() - pad - inset - knob / 2, cy)?;
        let knob_ar = pixel(&ar, toggle.x() + pad + inset + knob / 2, cy)?;
        let far_ar = pixel(&ar, toggle.x() + pad + track_w - inset - knob / 2, cy)?;
        check(knob_ar == knob_en && far_ar != knob_ar, &format!("ar: switch knob on the left ({knob_ar:?} = {knob_en:?}, other end {far_ar:?})"))?;
        // And back, still without rebuilding.
        i18n::set_locale("en");
        win.snapshot(shots.join("rtl-live-back.png")).map_err(|e| e.to_string())?;
        check(a.x() == ax && a.x() < c.x() && act.x() > win.w() / 2, "back to left to right")
    });
    run("rtl: Left/Right follow the screen (tabs, button group, slider, grid panel, tree); Tab goes start to end", &|shots| {
        i18n::set_locale("ar");
        let mut win = AppWindow::new("t", 600, 420);
        let mut body = win.body();
        let mut r = layout::row();
        let first = Button::new("first");
        let second = Button::new("second");
        r.fit(&*first);
        r.fit(&*second);
        r.close_fitted();
        let tab = State::new(0usize);
        let tabs = TabBar::new(&["one", "two", "three"], &tab);
        body.fit(&*tabs);
        let view = State::new(0usize);
        let group = ButtonGroup::new(&["Day", "Week", "Month"], &view);
        body.fit(&*group);
        let vol = State::new(0.5);
        let slider = Slider::new(&vol, 0.0, 1.0, 0.1);
        body.fit(&*slider);
        let grid = KeyboardPanel::new("grid").layout(PanelLayout::Grid { columns: 3 });
        grid.set_items(9, |i| i as u64, |id| (id < 9).then_some(id as usize));
        let mut canvas = fltk::widget::Widget::default().with_size(200, 40);
        canvas.handle(|_, ev| matches!(ev, fltk::enums::Event::Focus | fltk::enums::Event::Unfocus));
        body.fit(&canvas);
        focus::host(&canvas, &[&grid]);
        let tp = KeyboardPanel::new("tree");
        let tree = Tree::new(&tp, |p| match p {
            None => vec![TreeNode::new(1, "root").branch()],
            Some(1) => vec![TreeNode::new(2, "child")],
            _ => vec![],
        });
        body.fit(&*tree);
        body.fixed(&*tree, 80);
        win.end();
        win.snapshot(shots.join("rtl-keys-ar.png")).map_err(|e| e.to_string())?;
        check(first.x() > second.x(), "row mirrored")?;
        focus::first(&*win).ok_or("nothing focusable")?;
        check(focus::is_focused(&*first), "the first stop is the rightmost control")?;
        press("Tab");
        check(focus::is_focused(&*second), "Tab goes on to the left")?;
        press("Tab");
        check(focus::is_focused(&*tabs), "then the tab bar")?;
        let (r0, r1) = (tabs.tab_rect(0).ok_or("tab 0")?, tabs.tab_rect(1).ok_or("tab 1")?);
        check(r0.0 > r1.0, &format!("first tab on the right ({r0:?} {r1:?})"))?;
        press("Left");
        check(tab.get() == 1, &format!("Left goes to the next tab ({})", tab.get()))?;
        press("Right");
        check(tab.get() == 0, "Right goes back")?;
        press("Tab");
        check(focus::is_focused(&*group.button(0).unwrap()), "button group")?;
        press("Left");
        check(view.get() == 1, "Left picks the next option")?;
        press("Tab");
        check(focus::is_focused(&*slider), "slider")?;
        press("Left");
        check(vol.get() > 0.5, &format!("Left raises a mirrored slider ({})", vol.get()))?;
        press("Tab");
        check(grid.has_focus(), "grid panel")?;
        press("Left");
        check(grid.cursor_index() == Some(0), "first key lands on the first item")?;
        press("Left");
        check(grid.cursor_index() == Some(1), &format!("Left moves to the next item ({:?})", grid.cursor_index()))?;
        press("Right");
        check(grid.cursor_index() == Some(0), "Right moves back")?;
        press("Tab");
        check(tp.has_focus(), "tree panel")?;
        press("Down");
        check(tp.cursor() == Some(1), "tree cursor on the root")?;
        press("Left");
        check(tree.is_expanded(1), "Left expands in a right-to-left tree")?;
        press("Right");
        check(!tree.is_expanded(1), "Right collapses")?;
        i18n::set_locale("en");
        Ok(())
    });
    run("rtl: list scrollbar, popup menu glyphs, toast corner, rich text bidi", &|shots| {
        i18n::set_locale("ar");
        let mut win = AppWindow::new("t", 520, 520);
        let mut body = win.body();
        let anchor = Button::new("menu").bordered();
        body.fit(&*anchor);
        let list = List::new(&State::new(500usize), |i| ListRow::new(&format!("row {i}")));
        body.fit(&*list);
        body.fixed(&*list, 120);
        let rt = RichText::new().padding(0);
        body.fit(&*rt);
        body.fixed(&*rt, 160);
        win.end();
        rt.set_markdown("نص عربي مع كلمة word ورقم 2026.\n\nplain English line");
        let p = shots.join("rtl-parts-ar.png");
        win.snapshot(&p).map_err(|e| e.to_string())?;
        let bg = pixel(&p, 2, win.h() - 3)?;
        // Scrollbar thumb on the left edge of the list.
        let ty = list.y() + 10;
        check(pixel(&p, list.x() + 1, ty)? != bg, "ar: list scrollbar on the left")?;
        check(pixel(&p, list.x() + list.w() - 2, ty)? == bg, "ar: nothing at the right edge")?;
        // Rich text: the Arabic paragraph is right-aligned, "word" sits left of
        // the Arabic words before it, the English paragraph stays left-aligned.
        let arabic = rt.text_rect("نص").ok_or("arabic rect")?;
        let word = rt.text_rect("word").ok_or("word rect")?;
        let plain = rt.text_rect("plain").ok_or("plain rect")?;
        let mid = rt.x() + rt.w() / 2;
        check(arabic.0 > mid, &format!("Arabic paragraph starts on the right ({arabic:?})"))?;
        check(word.0 + word.2 <= arabic.0, &format!("Latin word after the Arabic text, to its left ({word:?} {arabic:?})"))?;
        check(plain.0 < mid, &format!("English paragraph on the left ({plain:?})"))?;
        // Popup menu: glyph and label on the right, nothing in the left half.
        let h = PopupMenu::open(&*anchor, vec![PopupItem::new(glyph::FOLDER, "فتح"), PopupItem::new(glyph::TRASH, "حذف")], |_| {});
        let (mx, my, mw, _) = h.rect().ok_or("menu rect")?;
        let pm = shots.join("rtl-popup-ar.png");
        win.snapshot(&pm).map_err(|e| e.to_string())?;
        // The second entry (the first one has the hover fill).
        let row_h = theme::current().spacing.popup_row_height;
        let top = my + 2 + theme::current().spacing.xxs + row_h;
        let fill = pixel(&pm, mx + mw / 2, top + 2)?;
        let (mut left, mut right) = (0, 0);
        for yy in (top + 3..top + row_h - 3).step_by(2) {
            for xx in (mx + 4..mx + mw / 2).step_by(2) {
                if pixel(&pm, xx, yy)? != fill { left += 1; }
            }
            for xx in (mx + mw / 2..mx + mw - 4).step_by(2) {
                if pixel(&pm, xx, yy)? != fill { right += 1; }
            }
        }
        check(right > 10 && left == 0, &format!("ar: menu entry on the right ({left} left, {right} right)"))?;
        h.close();
        let toast = OperationToast::show(&*win, "نسخ").ok_or("toast")?;
        let (tx, _, tw, _) = toast.rect();
        check(tx + tw < win.w() / 2 + tw / 2 && tx < win.w() - tx - tw, &format!("ar: operation toast at the bottom-left ({tx})"))?;
        toast.close();
        i18n::set_locale("en");
        Ok(())
    });
    run("rich text: bold ink, code chip, link underline, code surface, selection color", &|shots| {
        let mut win = AppWindow::new("t", 520, 300);
        let rt = RichText::new();
        win.end();
        rt.set_markdown("same **same** `chip` [link](https://example.com) plain words\n\n```\nlet x = 1;\n```\n");
        let p = shots.join("richtext.png");
        win.snapshot(&p).map_err(|e| e.to_string())?;
        let t = theme::current();
        let bg = t.palette.background;
        let ink = |p: &Path, (x, y, w, h): (i32, i32, i32, i32)| -> Result<usize, String> {
            let mut n = 0;
            for yy in y..y + h {
                for xx in x..x + w {
                    let px = pixel(p, xx, yy)?;
                    let d = (px[0] as i32 - bg.r() as i32).abs() + (px[1] as i32 - bg.g() as i32).abs() + (px[2] as i32 - bg.b() as i32).abs();
                    if d > 60 { n += 1; }
                }
            }
            Ok(n)
        };
        let plain = rt.text_rect("same").ok_or("plain rect")?;
        let b0 = rt.text_rect("same chip").ok_or("bold rect")?;
        let b1 = rt.text_rect(" chip").ok_or("chip rect")?;
        let bold = (b0.0, b0.1, b1.0 - b0.0, b0.3);
        let (ip, ib) = (ink(&p, plain)?, ink(&p, bold)?);
        // Bold faces add less ink on macOS/Windows (Menlo, Consolas) than fontconfig's.
        let more = if OS == Os::Linux { 110 } else { 104 };
        check(ib * 100 > ip * more, &format!("bold ink {ib} vs regular {ip}"))?;
        let chip = rt.text_rect("chip").ok_or("chip")?;
        let px = pixel(&p, chip.0 - 1, chip.1 + chip.3 / 2)?;
        let lb = t.palette.lighter_background;
        check(px == [lb.r(), lb.g(), lb.b()], &format!("chip {px:?} vs lighter_background {:?}", lb.hex()))?;
        let link = rt.text_rect("link").ok_or("link")?;
        let a = t.palette.accent;
        let underline = (link.1..link.1 + link.3).any(|y| pixel(&p, link.0 + link.2 / 2, y).is_ok_and(|px| px == [a.r(), a.g(), a.b()]));
        check(underline, "link underline in the accent color")?;
        let code = rt.text_rect("let x").ok_or("code")?;
        let px = pixel(&p, code.0 - 3, code.1 + code.3 / 2)?;
        check(px == [lb.r(), lb.g(), lb.b()], &format!("code surface {px:?}"))?;
        // Selection "same " paints the space between the two words.
        rt.set_selection(TextPos { block: 0, offset: 0 }, TextPos { block: 0, offset: 5 });
        let p2 = shots.join("richtext-selection.png");
        win.snapshot(&p2).map_err(|e| e.to_string())?;
        let gap_x = (plain.0 + plain.2 + bold.0) / 2;
        let sc = t.palette.foreground.over(bg, t.controls.selection_fill_alpha);
        let px = pixel(&p2, gap_x, plain.1 + plain.3 / 2)?;
        check(px == [sc.r(), sc.g(), sc.b()], &format!("selection {px:?} vs {:?}", sc.hex()))?;
        check(rt.selected_text() == "same ", "selected text")
    });
    run("rich text: 5 000 messages are virtual; sticks to the bottom while streaming; keys scroll", &|shots| {
        let mut win = AppWindow::new("t", 480, 360);
        let rt = RichText::new().stick_to_bottom(true);
        win.end();
        for k in 0..5000 {
            rt.append_markdown(&format!("**Message {k}**\n\nSome text with `code` and a [link](https://x.dev/{k})."));
        }
        let p = shots.join("richtext-chat.png");
        win.snapshot(&p).map_err(|e| e.to_string())?;
        let laid = rt.blocks_laid_out();
        check(rt.block_count() == 10_000, "blocks")?;
        check(laid < 80, &format!("laid out {laid} of 10 000 blocks"))?;
        check(rt.is_at_bottom() && rt.text_rect("Message 4999").is_some(), "starts at the bottom")?;
        rt.append_markdown("");
        let mut md = String::new();
        for k in 0..120 {
            let piece = if k % 15 == 14 { "\n\n".to_string() } else { format!("token{k} ") };
            md.push_str(&piece);
            rt.push_markdown(&piece);
            if k % 4 == 0 { win.snapshot(&p).map_err(|e| e.to_string())?; }
        }
        win.snapshot(&p).map_err(|e| e.to_string())?;
        check(rt.is_at_bottom() && rt.text_rect("token118").is_some(), "follows the stream")?;
        check(rt.blocks_laid_out() - laid < 200, &format!("relaid {} blocks for 120 pushes", rt.blocks_laid_out() - laid))?;
        // The streamed message equals a one-shot parse of the same text.
        let one = fltk_omarchy::text::Document::from_markdown(&md).blocks;
        let base = rt.block_count() - one.len();
        check((0..one.len()).all(|i| rt.block(base + i).as_ref() == Some(&one[i])), "stream == one-shot")?;
        let mut w = (*rt).clone();
        let _ = w.take_focus();
        press("Home");
        win.snapshot(&p).map_err(|e| e.to_string())?;
        check(rt.scroll_offset() == 0 && rt.text_rect("Message 0").is_some(), "Home")?;
        press("PageDown");
        check(rt.scroll_offset() > 200, "PageDown")?;
        rt.push_markdown("more ");
        win.snapshot(&p).map_err(|e| e.to_string())?;
        check(!rt.is_at_bottom(), "scrolled up: no longer follows")?;
        press("End");
        rt.push_markdown("\n\nend of stream");
        win.snapshot(&p).map_err(|e| e.to_string())?;
        check(rt.is_at_bottom() && rt.text_rect("end of stream").is_some(), "End follows again")
    });
    run("rich text: links by click and by Tab / Enter, copy button stop, Ctrl+A, Esc", &|shots| {
        let opened = Rc::new(RefCell::new(Vec::<String>::new()));
        let mut win = AppWindow::new("t", 480, 320);
        let mut body = win.body();
        let before = Button::new("Before");
        body.fit(&*before);
        let o = opened.clone();
        let rt = RichText::new().on_link(move |u| o.borrow_mut().push(u.to_string()));
        let after = Button::new("After");
        body.fit(&*after);
        win.end();
        rt.set_markdown("Read [one](https://one.dev) and [two](https://two.dev).\n\n```\ncode\n```\n");
        win.snapshot(shots.join("richtext-links.png")).map_err(|e| e.to_string())?;
        let (x, y, w, h) = rt.text_rect("one").ok_or("link rect")?;
        focus::click_at(&*win, x + w / 2, y + h / 2);
        check(*opened.borrow() == ["https://one.dev"], &format!("click opened {:?}", opened.borrow()))?;
        let _ = before.clone().take_focus();
        press("Tab");
        check(focus::is_focused(&*rt) && rt.focused_link().is_none(), "Tab enters the view")?;
        press("Tab");
        check(rt.focused_link().as_deref() == Some("https://one.dev"), "Tab: first link")?;
        press("Tab");
        check(rt.focused_link().as_deref() == Some("https://two.dev"), "Tab: second link")?;
        win.snapshot(shots.join("richtext-link-focus.png")).map_err(|e| e.to_string())?;
        press("Enter");
        check(opened.borrow().last().map(String::as_str) == Some("https://two.dev"), "Enter opens it")?;
        press("Tab");
        check(focus::is_focused(&*rt) && rt.focused_link().is_none(), "Tab: code copy button")?;
        press("Tab");
        check(focus::is_focused(&*after), "Tab leaves the view")?;
        press("Shift+Tab");
        check(focus::is_focused(&*rt), "Shift+Tab comes back")?;
        check(press("Mod+a") && rt.selected_text() == rt.plain_text(), "Mod+A (Ctrl+A, Cmd+A on macOS) selects all")?;
        check(rt.plain_text() == "Read one and two.\ncode", "plain text")?;
        check(press("Mod+c"), "Mod+C copies")?;
        check(press("Escape") && rt.selection().is_none(), "Esc clears")
    });
    run("rich text: a theme change re-colors without parsing or laying out again", &|shots| {
        let mut win = AppWindow::new("t", 400, 200);
        let rt = RichText::new();
        win.end();
        rt.set_markdown("A [link](https://x.dev) here.");
        let p = shots.join("richtext-theme.png");
        win.snapshot(&p).map_err(|e| e.to_string())?;
        let laid = rt.blocks_laid_out();
        let old = theme::current();
        let mut t = old.clone();
        t.palette.accent = theme::Rgb(0x33ccff);
        theme::set(t);
        win.snapshot(&p).map_err(|e| e.to_string())?;
        let link = rt.text_rect("link").ok_or("link")?;
        let under = (link.1..link.1 + link.3).any(|y| pixel(&p, link.0 + 2, y).is_ok_and(|px| px == [0x33, 0xcc, 0xff]));
        theme::set(old);
        check(under, "underline in the new accent")?;
        check(rt.blocks_laid_out() == laid, "no relayout for a color change")
    });
    run("rich text: bind_markdown follows appends and replacements; sender() from a thread", &|shots| {
        let md = State::new("# Title\n\nfirst".to_string());
        let mut win = AppWindow::new("t", 400, 240);
        let rt = RichText::new().bind_markdown(&md);
        win.end();
        win.snapshot(shots.join("richtext-bind.png")).map_err(|e| e.to_string())?;
        check(rt.plain_text() == "Title\nfirst", &format!("bound {:?}", rt.plain_text()))?;
        let laid = rt.blocks_laid_out();
        md.update(|s| s.push_str(" more\n\n- item"));
        win.snapshot(shots.join("richtext-bind.png")).map_err(|e| e.to_string())?;
        check(rt.plain_text() == "Title\nfirst more\nitem", &format!("appended {:?}", rt.plain_text()))?;
        // The heading did not change: only the paragraph and the item were laid out.
        check(rt.blocks_laid_out() - laid == 2, &format!("relaid {}", rt.blocks_laid_out() - laid))?;
        // State::push_str carries Edit::Appended; a text feed batches chunks into one.
        let laid = rt.blocks_laid_out();
        md.push_str(" again");
        win.snapshot(shots.join("richtext-bind.png")).map_err(|e| e.to_string())?;
        check(rt.plain_text() == "Title\nfirst more\nitem again" && rt.blocks_laid_out() - laid == 1, &format!("push_str {:?} relaid {}", rt.plain_text(), rt.blocks_laid_out() - laid))?;
        let feed = md.text_feed();
        std::thread::spawn(move || { for c in [" and", " fed"] { feed.push_str(c); } }).join().map_err(|_| "feed thread".to_string())?;
        pump_until(|| rt.plain_text().ends_with("fed"));
        check(rt.plain_text() == "Title\nfirst more\nitem again and fed", &format!("text_feed {:?}", rt.plain_text()))?;
        md.set("Other *text*".into());
        check(rt.plain_text() == "Other text" && rt.block_count() == 1, "replaced")?;
        let tx = rt.sender();
        let worker = std::thread::spawn(move || {
            for w in ["\n\nfrom ", "a **worker** ", "thread"] {
                tx.push(w);
            }
            tx.finish();
        });
        worker.join().map_err(|_| "worker panicked".to_string())?;
        pump_until(|| rt.plain_text().ends_with("thread"));
        check(rt.plain_text() == "Other text\nfrom a worker thread", &format!("sender {:?}", rt.plain_text()))
    });
    run("rich label: inline markdown, natural size, link click", &|shots| {
        let opened = Rc::new(RefCell::new(String::new()));
        let mut win = AppWindow::new("t", 420, 120);
        let o = opened.clone();
        let mut r = layout::row();
        let l = RichLabel::new("Saved **3 files** to `~/x` — [undo](app://undo)").on_link(move |u| *o.borrow_mut() = u.into());
        r.fit(&*l);
        r.close_fitted();
        win.end();
        win.snapshot(shots.join("rich-label.png")).map_err(|e| e.to_string())?;
        check(l.text() == "Saved 3 files to ~/x — undo", "text")?;
        check(l.w() > 150 && l.h() >= 16, &format!("natural size {}x{}", l.w(), l.h()))?;
        // "undo" is the last word: click near the right end of the label.
        focus::click_at(&*win, l.x() + l.w() - 12, l.y() + l.h() / 2);
        check(*opened.borrow() == "app://undo", &format!("clicked {:?}", opened.borrow()))?;
        let blocks = [Block::paragraph("x").push("y", &Style::plain().color(ColorRole::Red))];
        check(blocks[0].content.runs().len() == 2, "builder runs")
    });
    tooltip_ui::checks(&mut run, COLORS_A, COLORS_B);
    edit_ui::checks(&mut run);
    motion_ui::checks(&mut run);
    #[cfg(feature = "a11y")]
    a11y_checks(&mut run);
    #[cfg(feature = "a11y")]
    edit_ui::a11y_checks(&mut run);
    #[cfg(feature = "a11y")]
    motion_ui::a11y_checks(&mut run);
    drop(app);
    println!("{}", if failed == 0 { "all ui checks passed" } else { "ui checks FAILED" });
    if failed > 0 && !strict {
        println!("({failed} failure(s) not fatal on {}: pixel expectations are calibrated on Linux; OMARCHY_FLTK_UI_STRICT=1 makes them fatal)", OS.name());
        std::process::exit(0);
    }
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

struct Rgb { w: usize, depth: usize, raw: Vec<u8> }
impl Rgb {
    fn at(&self, x: i32, y: i32) -> [u8; 3] {
        let off = y as usize * (self.w * self.depth + 1) + 1 + x as usize * self.depth;
        [self.raw[off], self.raw[off + 1], self.raw[off + 2]]
    }
}

/// Decodes a whole PNG written by our encoder (stored deflate).
fn png_rgb(path: &Path) -> Result<Rgb, String> {
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
    Ok(Rgb { w, depth, raw })
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

// ================================================================ a11y

/// Applies every update to AccessKit's own consumer, which panics on an
/// inconsistent tree exactly like the platform adapters would.
#[cfg(feature = "a11y")]
use fltk_omarchy::a11y::testing::{accesskit, accesskit_consumer};

#[cfg(feature = "a11y")]
struct Consumer(Option<accesskit_consumer::Tree>);

#[cfg(feature = "a11y")]
impl Consumer {
    fn feed(&mut self, updates: Vec<accesskit::TreeUpdate>) -> usize {
        struct Nop;
        impl accesskit_consumer::TreeChangeHandler for Nop {
            fn node_added(&mut self, _: &accesskit_consumer::NodeRef) {}
            fn node_updated(&mut self, _: &accesskit_consumer::NodeRef, _: &accesskit_consumer::NodeRef) {}
            fn focus_moved(&mut self, _: Option<&accesskit_consumer::NodeRef>, _: Option<&accesskit_consumer::NodeRef>) {}
            fn node_removed(&mut self, _: &accesskit_consumer::NodeRef) {}
        }
        let mut sent = 0;
        for u in updates {
            sent += u.nodes.len();
            match &mut self.0 {
                None => self.0 = Some(accesskit_consumer::Tree::new(u, true)),
                Some(t) => t.update_and_process_changes(u, &mut Nop),
            }
        }
        sent
    }

    fn focus_label(&self) -> Option<String> {
        self.0.as_ref()?.state().focus()?.label()
    }
}

#[cfg(feature = "a11y")]
type Check<'a> = &'a dyn Fn(&Path) -> Result<(), String>;

#[cfg(feature = "a11y")]
fn a11y_checks(run: &mut dyn FnMut(&str, Check)) {
    use accesskit::{Action as A, ActionData, Role as R};
    use fltk_omarchy::a11y::{self, testing, AccessibleExt, Live};

    run("a11y: sample window tree (roles, names, values, states, focus)", &|shots| {
        let mut win = AppWindow::new("Settings", 520, 900);
        win.header().action(glyph::SETTINGS, "Preferences", || {});
        let mut body = win.body();
        let name = State::new("Ada".to_string());
        body.fit(&*TextField::new().placeholder("Name").bind(&name));
        // Kept alive here: a derived State dropped right away stops updating.
        let greet = name.map(|n| format!("Hello {n}"));
        body.fit(&*Label::new("").bind(&greet));
        let clicks = State::new(0u32);
        let c = clicks.clone();
        body.fit(&*Button::new("Add").on_click(move || c.update(|v| *v += 1)));
        body.fit(&*Button::icon(glyph::TRASH, "").accessible_name("Delete"));
        let wifi = State::new(true);
        body.fit(&*ToggleSwitch::new(&wifi).accessible_name("Wi-Fi"));
        let night = State::new(false);
        body.fit(&*Toggle::new("Night light", "Warm colors at night", &night));
        let vol = State::new(0.5);
        body.fit(&*Slider::new(&vol, 0.0, 1.0, 0.1).accessible_name("Volume"));
        let copies = State::new(3i64);
        body.fit(&*NumberField::new(&copies, 0, 10, 1).accessible_name("Copies"));
        let out = State::new(1usize);
        body.fit(&*Dropdown::new("Output", &["Speakers", "Headphones"], &out));
        let tab = State::new(0usize);
        body.fit(&*TabBar::new(&["General", "Advanced"], &tab));
        let count = State::new(100_000usize);
        body.fit(&*List::new(&count, |i| ListRow::new(&format!("Item {i}")).detail("KB")));
        win.end();
        win.snapshot(shots.join("a11y.png")).map_err(|e| e.to_string())?;
        let rec = testing::record(&*win);
        rec.sync();
        let mut consumer = Consumer(None);
        let full = consumer.feed(rec.take());
        let snap = a11y::snapshot(&*win).unwrap();
        std::fs::write(shots.join("a11y-tree.txt"), &snap).map_err(|e| e.to_string())?;
        for want in [
            "Window \"Settings\"", "Heading \"Settings\" [level 1]", "Toolbar", "Button \"Preferences\"",
            "TextInput \"Name\" = \"Ada\"", "Label = \"Hello Ada\"", "Button \"Add\"", "Button \"Delete\"",
            "Switch \"Wi-Fi\" [checked]", "Switch \"Night light\" [unchecked, desc \"Warm colors at night\"]",
            "Slider \"Volume\" = \"0.50\"", "SpinButton \"Copies\" = \"3\"", "ComboBox \"Output\" = \"Headphones\" [collapsed]",
            "TabList", "Tab \"General\" [selected, 1 of 2]", "Tab \"Advanced\" [2 of 2]", "ListBox", "ListBoxOption \"Item 0\" [1 of 100000",
        ] {
            check(snap.contains(want), &format!("tree has {want:?}:\n{snap}"))?;
        }
        let rows = snap.lines().filter(|l| l.contains("ListBoxOption")).count();
        check(rows > 0 && rows <= 20, &format!("virtual list exposes the rows in view, not 100000 ({rows})"))?;
        check(rec.find(R::TextRun, "").is_none(), "text runs are not named")?;

        // Focus follows the keyboard router.
        focus::first(&*win);
        rec.sync();
        consumer.feed(rec.take());
        check(consumer.focus_label().as_deref() == Some("Preferences"), &format!("first focus: {:?}", consumer.focus_label()))?;
        press("Tab");
        rec.sync();
        consumer.feed(rec.take());
        check(consumer.focus_label().as_deref() == Some("Name"), &format!("Tab focus: {:?}", consumer.focus_label()))?;

        // A State change sends only the nodes that changed.
        name.set("Grace".into());
        rec.sync();
        let ups = rec.take();
        check(ups.len() == 1 && ups[0].tree.is_none(), "one incremental update")?;
        let changed: Vec<_> = ups[0].nodes.iter().map(|(_, n)| (n.role(), n.value().map(str::to_string))).collect();
        check(changed.contains(&(R::TextInput, Some("Grace".into()))) && changed.contains(&(R::Label, Some("Hello Grace".into()))), &format!("changed nodes: {changed:?}"))?;
        let sent = consumer.feed(ups);
        check(sent < full / 4, &format!("incremental ({sent} nodes) is much smaller than the full tree ({full})"))?;

        // Actions from the AT run the keyboard code paths.
        let add = rec.find(R::Button, "Add").ok_or("Add button")?;
        rec.act(add, A::Click, None);
        check(clicks.get() == 1, "AT click presses Add")?;
        rec.act(rec.find(R::Switch, "Wi-Fi").ok_or("wifi")?, A::Click, None);
        check(!wifi.get(), "AT click toggles the switch")?;
        rec.act(rec.find(R::Slider, "Volume").ok_or("slider")?, A::Increment, None);
        check((vol.get() - 0.6).abs() < 1e-9, &format!("increment: {}", vol.get()))?;
        rec.act(rec.find(R::SpinButton, "Copies").ok_or("spin")?, A::SetValue, Some(ActionData::NumericValue(7.0)));
        check(copies.get() == 7, "spin button set value")?;
        rec.act(rec.find(R::TextInput, "Name").ok_or("field")?, A::SetValue, Some(ActionData::Value("Linus".into())));
        check(name.get() == "Linus", "set value reaches the State")?;
        rec.act(rec.find(R::Tab, "Advanced").ok_or("tab")?, A::Click, None);
        check(tab.get() == 1, "AT click selects a tab")?;
        rec.act(rec.find(R::ComboBox, "Output").ok_or("combo")?, A::Expand, None);
        consumer.feed(rec.take());
        let snap = a11y::snapshot(&*win).unwrap();
        check(snap.contains("ComboBox \"Output\" = \"Headphones\" [expanded]") && snap.contains("ListBoxOption \"Speakers\""), &format!("dropdown popup:\n{snap}"))?;
        check(consumer.focus_label().as_deref() == Some("Headphones"), &format!("popup focus: {:?}", consumer.focus_label()))?;
        rec.act(rec.find(R::ListBoxOption, "Speakers").ok_or("option")?, A::Click, None);
        consumer.feed(rec.take());
        let snap = a11y::snapshot(&*win).unwrap();
        check(out.get() == 0 && snap.contains("ComboBox \"Output\" = \"Speakers\" [focused, collapsed]"), &format!("picked from the AT:\n{snap}"))?;
        fltk::window::DoubleWindow::delete((*win).clone());
        Ok(())
    });

    run("a11y: menus, dialogs, toasts and announcements appear in the tree", &|_| {
        let mut win = AppWindow::new("Overlays", 640, 520);
        let mut body = win.body();
        let b = Button::new("Anchor");
        body.fit(&*b);
        win.end();
        let rec = testing::record(&*win);
        focus::first(&*win);
        rec.sync();
        let mut consumer = Consumer(None);
        consumer.feed(rec.take());
        let picked = Rc::new(Cell::new(None));
        let p2 = picked.clone();
        Menu::open(&*b, "Go", vec![MenuItem::new("", "Screenshot"), MenuItem::new("", "Record").detail("screen")], move |i| p2.set(Some(i)));
        rec.sync();
        consumer.feed(rec.take());
        let snap = a11y::snapshot(&*win).unwrap();
        check(snap.contains("Menu \"Go\"") && snap.contains("MenuItem \"Screenshot\" [focused, 1 of 2]"), &format!("menu:\n{snap}"))?;
        rec.act(rec.find(R::MenuItem, "Record").ok_or("Record")?, A::Click, None);
        consumer.feed(rec.take());
        check(picked.get() == Some(1) && !a11y::snapshot(&*win).unwrap().contains("Menu "), "AT pick closes the menu")?;

        let answer = Rc::new(Cell::new(None));
        let a2 = answer.clone();
        ConfirmDialog::open(&*b, "Delete 3 files?", "Delete", true, move |ok| a2.set(Some(ok)));
        rec.sync();
        consumer.feed(rec.take());
        let snap = a11y::snapshot(&*win).unwrap();
        check(snap.contains("AlertDialog \"Delete 3 files?\" [modal]") && snap.contains("Button \"Delete\" [focused]"), &format!("dialog:\n{snap}"))?;
        rec.act(rec.find(R::Button, "Delete").ok_or("Delete")?, A::Click, None);
        consumer.feed(rec.take());
        check(answer.get() == Some(true), "AT confirms the dialog")?;
        check(consumer.focus_label().as_deref() == Some("Anchor"), &format!("focus back: {:?}", consumer.focus_label()))?;

        let picked = Rc::new(Cell::new(None));
        let p3 = picked.clone();
        let handle = PopupMenu::open(&*b, vec![PopupItem::new("", "Copy").shortcut("Ctrl+C"), PopupItem::new("", "Paste").disabled(), PopupItem::new("", "Wrap").checked(true)],
                                     move |i| p3.set(Some(i)));
        rec.sync();
        consumer.feed(rec.take());
        let snap = a11y::snapshot(&*win).unwrap();
        for want in ["MenuItem \"Copy\" [focused, 1 of 3]", "MenuItem \"Paste\" [disabled, 2 of 3]", "MenuItemCheckBox \"Wrap\" [checked, 3 of 3]"] {
            check(snap.contains(want), &format!("popup menu has {want:?}:\n{snap}"))?;
        }
        rec.act(rec.find(R::MenuItemCheckBox, "Wrap").ok_or("Wrap")?, A::Click, None);
        consumer.feed(rec.take());
        check(picked.get() == Some(2) && handle.dismissed() == Some(Dismiss::Picked(2)), "AT pick in a popup menu")?;

        let toasts = Notification::count();
        Notification::show(&*b, "", "Saved", "3 files", Urgency::Normal);
        let mut toast = OperationToast::show(&*b, "Copying").ok_or("toast")?;
        toast.set_progress(0.25);
        toast.set_detail("2 of 8");
        a11y::announce("3 results", Live::Polite);
        rec.sync();
        consumer.feed(rec.take());
        let snap = a11y::snapshot(&*win).unwrap();
        for want in ["Alert \"Saved\" [desc \"3 files\", live]", "Status \"Copying\" [desc \"2 of 8\", live]", "ProgressIndicator \"Copying\" = \"25%\"",
                     "Button \"Pause\"", "Button \"Cancel\"", "Status \"3 results\" [live]"] {
            check(snap.contains(want), &format!("toasts have {want:?}:\n{snap}"))?;
        }
        rec.act(rec.find(R::Button, "Pause").ok_or("Pause")?, A::Click, None);
        consumer.feed(rec.take());
        check(toast.is_paused() && rec.find(R::Button, "Resume").is_some(), "AT pauses the operation")?;
        rec.act(rec.find(R::Alert, "Saved").ok_or("alert")?, A::Click, None);
        consumer.feed(rec.take());
        check(Notification::count() == toasts, "AT click dismisses the notification")?;
        toast.close();
        fltk::window::DoubleWindow::delete((*win).clone());
        Ok(())
    });

    run("a11y: table, tree, keyboard panels and plain fltk widgets", &|shots| {
        let mut win = AppWindow::new("Data", 640, 720);
        let mut body = win.body();
        let tp = KeyboardPanel::new("files");
        tp.set_items(5, |i| 100 + i as u64, |id| (100..105).contains(&id).then(|| (id - 100) as usize));
        let table = Table::new(&tp, vec![Column::new("Name", 0).flex(), Column::new("Size", 90).right()], |r, c| if c == 0 { format!("file{r}") } else { format!("{} KB", r * 2) });
        body.fit(&*table);
        let trp = KeyboardPanel::new("tree");
        let tree = Tree::new(&trp, |p| match p {
            None => vec![TreeNode::new(1, "Documents").branch(), TreeNode::new(2, "Music")],
            Some(1) => vec![TreeNode::new(11, "notes.md"), TreeNode::new(12, "todo.md")],
            _ => vec![],
        });
        body.fit(&*tree);
        let left = KeyboardPanel::new("Places");
        left.set_items(3, |i| 7 + i as u64, |id| (7..10).contains(&id).then(|| (id - 7) as usize));
        left.accessible_labels(|id| ["Home", "Downloads", "Trash"][(id - 7) as usize].to_string());
        let mut canvas = fltk::widget::Widget::default().with_size(200, 80);
        canvas.handle(|_, ev| matches!(ev, fltk::enums::Event::Focus | fltk::enums::Event::Unfocus));
        focus::host(&canvas, &[&left]);
        body.fit(&canvas);
        let hits = Rc::new(Cell::new(0));
        let h2 = hits.clone();
        let mut plain = fltk::button::Button::default().with_size(80, 28).with_label("&Plain");
        plain.set_callback(move |_| h2.set(h2.get() + 1));
        body.fit(&plain);
        let mut check_b = fltk::button::CheckButton::default().with_size(120, 28).with_label("Hidden files");
        check_b.set_value(true);
        body.fit(&check_b);
        let input = fltk::input::Input::default().with_size(160, 28).with_label("Path");
        body.fit(&input);
        win.end();
        win.snapshot(shots.join("a11y-data.png")).map_err(|e| e.to_string())?;
        let rec = testing::record(&*win);
        rec.sync();
        let mut consumer = Consumer(None);
        consumer.feed(rec.take());
        let snap = a11y::snapshot(&*win).unwrap();
        for want in ["Grid [6x2]", "ColumnHeader \"Name\"", "Row \"file0\"", "Cell \"4 KB\"", "Tree", "TreeItem \"Documents\" [collapsed, 1 of 2, level 1]",
                     "Group", "ListBox \"Places\"", "ListBoxOption \"Home\" [1 of 3]", "Button \"Plain\"", "CheckBox \"Hidden files\" [checked]", "TextInput \"Path\" = \"\""] {
            check(snap.contains(want), &format!("tree has {want:?}:\n{snap}"))?;
        }
        // Keyboard: the table's cursor row is the focused node.
        tp.focus();
        press("Down");
        press("Down");
        rec.sync();
        consumer.feed(rec.take());
        check(consumer.focus_label().as_deref() == Some("file1"), &format!("table cursor focus: {:?}", consumer.focus_label()))?;
        rec.act(rec.find(R::Row, "file3").ok_or("row")?, A::Click, None);
        check(tp.selection() == vec![103], &format!("AT click selects by ID: {:?}", tp.selection()))?;
        rec.act(rec.find(R::ColumnHeader, "Size").ok_or("header")?, A::Click, None);
        check(table.sort() == Some((1, true)), "AT click on a header sorts")?;
        rec.act(rec.find(R::TreeItem, "Documents").ok_or("tree item")?, A::Expand, None);
        consumer.feed(rec.take());
        check(tree.is_expanded(1) && a11y::snapshot(&*win).unwrap().contains("TreeItem \"notes.md\" [1 of 2, level 2]"), "AT expands a tree item")?;
        // A canvas with keyboard panels: the cursor item is focused.
        left.focus();
        press("Down");
        press("Down");
        rec.sync();
        consumer.feed(rec.take());
        check(consumer.focus_label().as_deref() == Some("Downloads"), &format!("panel cursor focus: {:?}", consumer.focus_label()))?;
        rec.act(rec.find(R::ListBoxOption, "Trash").ok_or("Trash")?, A::Click, None);
        check(left.selection() == vec![9], "AT click on a panel item")?;
        rec.act(rec.find(R::Button, "Plain").ok_or("plain")?, A::Click, None);
        check(hits.get() == 1, "AT click runs a plain fltk button's callback")?;
        rec.act(rec.find(R::CheckBox, "Hidden files").ok_or("check")?, A::Click, None);
        check(!check_b.value(), "AT click toggles a plain check button")?;
        rec.act(rec.find(R::TextInput, "Path").ok_or("input")?, A::SetValue, Some(ActionData::Value("/tmp".into())));
        check(input.value() == "/tmp", "AT sets a plain input")?;
        fltk::window::DoubleWindow::delete((*win).clone());
        Ok(())
    });
    run("a11y: mirrored bounds in a right-to-left language", &|shots| {
        i18n::set_locale("ar");
        let mut win = AppWindow::new("t", 480, 240);
        win.header().action(glyph::SETTINGS, "Preferences", || {});
        let mut body = win.body();
        let tabs = TabBar::new(&["one", "two"], &State::new(0usize));
        body.fit(&*tabs);
        let list = List::new(&State::new(3usize), |i| ListRow::new(&format!("row {i}")));
        body.fit(&*list);
        win.end();
        win.snapshot(shots.join("a11y-rtl.png")).map_err(|e| e.to_string())?;
        let rec = testing::record(&*win);
        rec.sync();
        let mut consumer = Consumer(None);
        consumer.feed(rec.take());
        let rect = |role: R, name: &str| rec.find(role, name).and_then(|id| rec.node(id)).and_then(|n| n.bounds()).ok_or(format!("{name} bounds"));
        let (one, two) = (rect(R::Tab, "one")?, rect(R::Tab, "two")?);
        check(one.x0 > two.x0, &format!("first tab on the right ({} > {})", one.x0, two.x0))?;
        let pref = rect(R::Button, "Preferences")?;
        check(pref.x1 < 240.0, &format!("header action on the left ({})", pref.x1))?;
        let row = rect(R::ListBoxOption, "row 0")?;
        check((row.x1 - (list.x() + list.w()) as f64).abs() < 1.5, &format!("list rows end at the right edge ({} vs {})", row.x1, list.x() + list.w()))?;
        i18n::set_locale("en");
        fltk::window::DoubleWindow::delete((*win).clone());
        Ok(())
    });
    run("a11y: rich text is a document of the blocks near the view (roles, links, code, tables, stream, scroll)", &|shots| {
        let opened = Rc::new(RefCell::new(Vec::<String>::new()));
        let mut win = AppWindow::new("Chat", 560, 460);
        let mut body = win.body();
        let o = opened.clone();
        let rt = RichText::new().stick_to_bottom(true).on_link(move |u| o.borrow_mut().push(u.into())).accessible_name("Conversation");
        let o2 = opened.clone();
        let mut row = layout::row();
        row.fit(&*RichLabel::new("Saved **3 files** — [undo](app://undo)").on_link(move |u| o2.borrow_mut().push(u.into())));
        row.close_fitted();
        let _ = &mut body;
        win.end();
        for k in 0..2000 {
            rt.append_markdown(&format!("Message {k} with a [link](https://x.dev/{k})."));
        }
        rt.append_markdown("# Title\n\nSome **bold** text and [docs](https://docs.dev).\n\n1. one\n2. two\n3. three\n\n- [x] done\n\n```rust\nfn main() {}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n");
        win.snapshot(shots.join("a11y-richtext.png")).map_err(|e| e.to_string())?;
        let rec = testing::record(&*win);
        rec.sync();
        let mut consumer = Consumer(None);
        let full = consumer.feed(rec.take());
        let snap = a11y::snapshot(&*win).unwrap();
        std::fs::write(shots.join("a11y-richtext.txt"), &snap).map_err(|e| e.to_string())?;
        for want in [
            "Document \"Conversation\"", "Heading \"Title\" [level 1]", "Paragraph \"Some bold text and docs.\"", "Link \"docs\"",
            "ListItem \"two\" [2 of 3, level 1]", "ListItem \"done\" [checked, 1 of 1, level 1]", "Code \"fn main() {}\" [desc \"rust\"]",
            "Button \"Copy\"", "Table [2x2]", "ColumnHeader \"a\"", "Cell \"2\"", "Label = \"Saved 3 files — undo\"", "Link \"undo\"",
        ] {
            check(snap.contains(want), &format!("tree has {want:?}:\n{snap}"))?;
        }
        let paras = snap.lines().filter(|l| l.contains("Paragraph \"Message")).count();
        check(paras > 0 && paras < 60, &format!("only the blocks near the view: {paras} of 2000 messages"))?;
        let docs = rec.find(R::Link, "docs").ok_or("docs link")?;
        check(rec.node(docs).and_then(|n| n.url().map(str::to_string)).as_deref() == Some("https://docs.dev"), "link URL")?;
        // The document reads as text (AccessKit's text ranges over the runs below it).
        {
            let tree = consumer.0.as_ref().ok_or("tree")?;
            let state = tree.state();
            fn find<'a>(n: accesskit_consumer::NodeRef<'a>, role: R) -> Option<accesskit_consumer::NodeRef<'a>> {
                if n.role() == role { return Some(n); }
                n.children().find_map(|c| find(c, role))
            }
            let doc = find(state.root(), R::Document).ok_or("document node")?;
            check(doc.supports_text_ranges(), "document supports text ranges")?;
            let text = doc.document_range().text();
            check(text.contains("Some bold text and docs.") && text.contains("fn main() {}") && !text.contains("Message 0 "), &format!("document text {:?}", &text[text.len().saturating_sub(200)..]))?;
            let heading = find(doc, R::Heading).ok_or("heading")?;
            check(heading.bounding_box().is_some_and(|b| b.y0 >= 0.0 && b.y1 <= 460.0), &format!("heading on screen: {:?}", heading.bounding_box()))?;
        }
        // Click on a link opens it as Enter would; on the copy button it copies.
        rec.act(docs, A::Click, None);
        check(opened.borrow().last().map(String::as_str) == Some("https://docs.dev"), &format!("AT click opens the link: {:?}", opened.borrow()))?;
        rec.act(rec.find(R::Link, "undo").ok_or("label link")?, A::Click, None);
        check(opened.borrow().last().map(String::as_str) == Some("app://undo"), "AT click on a RichLabel link")?;
        rec.act(rec.find(R::Button, "Copy").ok_or("copy")?, A::Click, None);
        consumer.feed(rec.take());
        // Streaming into the last message updates only that block's nodes.
        rt.append_markdown("Streaming");
        win.snapshot(shots.join("a11y-richtext.png")).map_err(|e| e.to_string())?;
        rec.sync();
        consumer.feed(rec.take());
        rt.push_markdown(" more words");
        win.snapshot(shots.join("a11y-richtext.png")).map_err(|e| e.to_string())?;
        rec.sync();
        let ups = rec.take();
        let changed: Vec<String> = ups.iter().flat_map(|u| u.nodes.iter().map(|(_, n)| format!("{:?} {:?}", n.role(), n.label().or(n.value())))).collect();
        check(changed.iter().any(|c| c.contains("Streaming more words")), &format!("stream update: {changed:?}"))?;
        let sent = consumer.feed(ups);
        check(sent <= 8 && sent * 10 < full, &format!("a streamed token sends {sent} nodes (full tree {full}): {changed:?}"))?;
        // Scrolling moves the document node, not every block.
        let mut w = (*rt).clone();
        let _ = w.take_focus();
        press("Up");
        win.snapshot(shots.join("a11y-richtext.png")).map_err(|e| e.to_string())?;
        rec.sync();
        let ups = rec.take();
        let moved: Vec<String> = ups.iter().flat_map(|u| u.nodes.iter().map(|(_, n)| format!("{:?}", n.role()))).collect();
        check(moved.iter().filter(|r| *r == "Paragraph").count() <= 4, &format!("a scroll resends few blocks: {moved:?}"))?;
        consumer.feed(ups);
        // ScrollIntoView on a block above the view scrolls to it.
        let top = rt.scroll_offset();
        let first = snap.lines().find(|l| l.contains("Paragraph \"Message")).ok_or("a message")?;
        let name = first.trim().trim_start_matches("Paragraph ").trim_matches('"').to_string();
        rec.act(rec.find(R::Paragraph, &name).ok_or("first message")?, A::ScrollIntoView, None);
        check(rt.scroll_offset() < top, &format!("ScrollIntoView scrolled up: {top} -> {}", rt.scroll_offset()))?;
        // Tab to a link: the AT focus follows it.
        rt.scroll_to_bottom();
        win.snapshot(shots.join("a11y-richtext.png")).map_err(|e| e.to_string())?;
        let _ = w.take_focus();
        press("Tab");
        rec.sync();
        consumer.feed(rec.take());
        let f = consumer.focus_label();
        check(f.as_deref() == rt.focused_link().map(|u| if u.contains("docs") { "docs".to_string() } else { "link".to_string() }).as_deref() || f.as_deref() == Some("Copy"), &format!("focus on the Tab target: {f:?} ({:?})", rt.focused_link()))?;
        // Selection: the AT sees it on the document and can set it.
        rt.select_all();
        win.snapshot(shots.join("a11y-richtext.png")).map_err(|e| e.to_string())?;
        rec.sync();
        consumer.feed(rec.take());
        let doc = rec.find(R::Document, "Conversation").ok_or("doc")?;
        check(rec.node(doc).is_some_and(|n| n.text_selection().is_some()), "selection on the document")?;
        let para = rec.find(R::Paragraph, "Some bold text and docs.").ok_or("para")?;
        let run = rec.node(para).and_then(|n| n.children().iter().copied().find(|c| rec.node(*c).is_some_and(|r| r.role() == R::TextRun))).ok_or("run")?;
        let pos = |c| accesskit::TextPosition { node: run, character_index: c };
        rec.act(doc, A::SetTextSelection, Some(ActionData::SetTextSelection(accesskit::TextSelection { anchor: pos(5), focus: pos(9) })));
        check(rt.selected_text() == "bold", &format!("AT selection: {:?}", rt.selected_text()))?;
        consumer.feed(rec.take());
        fltk::window::DoubleWindow::delete((*win).clone());
        Ok(())
    });
}
