//! Every component in one window, for manual testing. Everything the user
//! does is logged with a timestamp to build/playground.log (and stderr):
//! clicks, state changes, focus moves, keyboard mode, window position and
//! size, native gestures and panics.
//!   cargo run --release --example playground [log path]
use fltk::prelude::*;
use fltk_omarchy::focus;
use fltk_omarchy::gfx::{self, Hit, Paint, Path, Rate, Stroke, View};
use fltk_omarchy::layout::{self, FlexExt};
use fltk_omarchy::list::{List, ListRow};
use fltk_omarchy::overlay::{ConfirmDialog, Dropdown, Menu, MenuItem, Notification, Urgency};
use fltk_omarchy::panel::KeyboardPanel;
use fltk_omarchy::popups::{OperationToast, PopupCard, PopupItem, PopupMenu, SearchableDropdown, ShortcutHelp};
use fltk_omarchy::table::{Column, Table, Tree, TreeNode};
use fltk_omarchy::task::{Outcome, Task};
use fltk_omarchy::theme::{self, Corners};
use fltk_omarchy::widgets::{glyph, *};
use fltk_omarchy::{hyprland, App, AppWindow, Mode, State};
use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

static LOG: OnceLock<Mutex<std::fs::File>> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();

fn log(msg: impl AsRef<str>) {
    let t = START.get().map_or(0.0, |s| s.elapsed().as_secs_f64());
    let line = format!("{t:8.3}  {}\n", msg.as_ref());
    eprint!("{line}");
    if let Some(f) = LOG.get() {
        let _ = f.lock().map(|mut f| f.write_all(line.as_bytes()));
    }
}

/// Names for the focus log.
type Names = Rc<RefCell<Vec<(usize, String)>>>;

fn name<W: WidgetExt>(names: &Names, w: &W, n: &str) {
    names.borrow_mut().push((w.as_widget_ptr() as usize, n.to_string()));
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "build/playground.log".into());
    START.set(Instant::now()).ok();
    LOG.set(Mutex::new(std::fs::File::create(&path).expect("log file"))).ok();
    std::panic::set_hook(Box::new(|info| log(format!("PANIC {info}"))));
    log(format!("playground start, pid {}", std::process::id()));

    let app = App::new("dev.omarchy.fltk.playground");
    let names: Names = Rc::default();
    let mut win = AppWindow::new("fltk-omarchy playground", 900, 680);
    win.set_mode(Mode::Floating);
    win.set_min_size(600, 480);
    let mut body = win.body();

    // ---- page tabs (8 tabs: the last ones overflow on narrow windows)
    let page = State::new(0usize);
    let page_names = ["Controls", "Lists", "Overlays", "Graphics", "Extra five", "Extra six", "Extra seven", "Extra eight"];
    let tabs = TabBar::new(&page_names, &page).on_close(|i| log(format!("tab close requested: {i}")));
    body.fit(&*tabs);
    name(&names, &*tabs, "tabs");

    // ==================================================== page 0: controls
    let mut p0 = layout::column();
    let mut r = layout::row();
    for (label, b) in [
        ("Default", Button::new("Default")),
        ("Bordered", Button::new("Bordered").bordered()),
        ("Primary", Button::icon(glyph::PLUS, "Primary").primary().shortcut("Ctrl+N")),
        ("Danger", Button::icon(glyph::TRASH, "Danger").danger().bordered()),
    ] {
        let l = label.to_string();
        let b = b.on_click(move || log(format!("button {l} clicked")));
        r.fit(&*b);
        name(&names, &*b, &format!("button {label}"));
    }
    let ab = ActionButton::new(glyph::REFRESH, "Refresh").on_click(|| log("action button Refresh"));
    r.fit(&*ab);
    name(&names, &*ab, "action Refresh");
    r.close_fitted();
    let period = State::new(1usize);
    period.subscribe(|v| log(format!("button group → {v}"))).forever();
    let mut r = layout::row();
    let bg = ButtonGroup::new(&["Day", "Week", "Month"], &period);
    r.fit(&*bg);
    let wifi = State::new(true);
    wifi.subscribe(|v| log(format!("switch wifi → {v}"))).forever();
    let sw = ToggleSwitch::new(&wifi);
    r.fit(&*sw);
    name(&names, &*sw, "switch");
    r.close_fitted();
    let night = State::new(false);
    night.subscribe(|v| log(format!("toggle night light → {v}"))).forever();
    let tg = Toggle::new("Night light", "Warmer colors after sunset", &night);
    p0.fit(&*tg);
    name(&names, &*tg, "toggle");
    let vol = State::new(0.4);
    vol.subscribe(|v| log(format!("slider → {v:.2}"))).forever();
    let mut r = layout::row();
    r.fit(&*Label::new("Volume"));
    let sl = Slider::new(&vol, 0.0, 1.0, 0.05);
    name(&names, &*sl, "slider");
    r.fit(&*Label::new("").right().bind(&vol.map(|v| format!("{:.0}%", v * 100.0))));
    r.close_fitted();
    p0.fit(&*Progress::new(&vol));
    let text = State::new(String::new());
    text.subscribe(|v| log(format!("text field → {v:?}"))).forever();
    let mut r = layout::row();
    let tf = TextField::new().placeholder("Type here…").bind(&text).on_submit(|v| log(format!("text submitted: {v:?}")));
    r.fit(&*tf);
    name(&names, &tf.input(), "text field");
    let pw = TextField::password().placeholder("Password").on_submit(|_| log("password submitted"));
    r.fit(&*pw);
    name(&names, &pw.input(), "password");
    let num = State::new(42i64);
    num.subscribe(|v| log(format!("number → {v}"))).forever();
    r.fit(&*NumberField::new(&num, 0, 100, 1));
    r.close_fitted();
    let out = State::new(0usize);
    out.subscribe(|v| log(format!("dropdown → {v}"))).forever();
    let place = State::new(None::<usize>);
    place.subscribe(|v| log(format!("searchable dropdown → {v:?}"))).forever();
    let mut r = layout::row();
    let dd = Dropdown::new("", &["Speakers", "Headphones", "HDMI"], &out);
    r.fit(&*dd);
    name(&names, &*dd, "dropdown");
    let sd = SearchableDropdown::new("Go to…", ["Home ~/", "Downloads ~/Downloads", "Work ~/Work", "Pictures ~/Pictures"].iter()
        .map(|s| { let (a, b) = s.split_once(' ').unwrap(); (a.to_string(), b.to_string()) }).collect(), &place);
    r.fit(&*sd);
    name(&names, &*sd, "searchable dropdown");
    r.close_fitted();
    for (g, l, d) in [(glyph::MONITOR, "HDMI / DisplayPort", "Digital"), (glyph::HEADPHONES, "Built-in speakers", "Analog")] {
        let ll = l.to_string();
        let row = Row::new(g, l).detail(d).on_click(move || log(format!("row clicked: {ll}")));
        p0.fit(&*row);
        name(&names, &*row, &format!("row {l}"));
    }
    layout::spacer();
    p0.end();

    // ======================================================= page 1: lists
    let p1 = layout::row();
    let mut c1 = layout::column();
    c1.fit(&*Label::section("List (100 000)"));
    let sel = State::new(None::<usize>);
    sel.subscribe(|v| log(format!("list selection → {v:?}"))).forever();
    let list = List::new(&State::new(100_000usize), |i| ListRow::new(&format!("Item {i}")).icon(glyph::FILE).detail(&format!("{} KB", i * 3)))
        .bind_selected(&sel)
        .on_activate(|i| log(format!("list activate {i}")));
    name(&names, &*list, "list");
    c1.end();
    let mut c2 = layout::column();
    c2.fit(&*Label::section("Table (10 000, multi-select, sort)"));
    let tp = KeyboardPanel::new("table");
    let n = 10_000usize;
    let order = Rc::new(RefCell::new((0..n).collect::<Vec<usize>>()));
    let set_items = {
        let (tp, order) = (tp.clone(), order.clone());
        move || {
            let (o1, o2) = (order.clone(), order.clone());
            tp.set_items(n, move |i| o1.borrow()[i] as u64, move |id| o2.borrow().iter().position(|&x| x as u64 == id));
        }
    };
    set_items();
    tp.on_activate(|id| log(format!("table activate id {id}")));
    tp.on_change({ let tp = tp.clone(); move || log(format!("table cursor {:?}, {} selected", tp.cursor(), tp.selection_len())) });
    let o3 = order.clone();
    let table = Table::new(&tp, vec![Column::new("Name", 0).flex(), Column::new("Size", 90).right(), Column::new("Kind", 70)],
        move |row, col| { let i = o3.borrow()[row]; match col { 0 => format!("file-{i:05}.txt"), 1 => format!("{} KB", (i * 37) % 9000), _ => "Text".into() } })
        .icon(|_| glyph::FILE.to_string())
        .on_sort({ let order = order.clone(); move |c, asc| {
            log(format!("table sort column {c} ascending {asc}"));
            let mut o = order.borrow_mut();
            match c { 1 => o.sort_by_key(|i| (i * 37) % 9000), _ => o.sort() }
            if !asc { o.reverse(); }
            drop(o);
            set_items();
        }});
    name(&names, &*table, "table");
    c2.end();
    let mut c3 = layout::column();
    c3.fit(&*Label::section("Tree (lazy)"));
    let trp = KeyboardPanel::new("tree");
    trp.on_activate(|id| log(format!("tree activate id {id}")));
    let tree = Tree::new(&trp, |parent| {
        log(format!("tree children requested for {parent:?}"));
        match parent {
            None => (0..4).map(|i| TreeNode::new(i, &format!("Folder {i}")).icon(glyph::FOLDER).branch()).collect(),
            Some(p) if p < 4 => (0..50).map(|j| TreeNode::new(100 + p * 100 + j, &format!("Item {p}.{j}")).icon(glyph::FILE).branch()).collect(),
            Some(p) => (0..3).map(|k| TreeNode::new(10_000 + p * 10 + k, &format!("Leaf {p}.{k}")).icon(glyph::FILE)).collect(),
        }
    });
    name(&names, &*tree, "tree");
    c3.end();
    p1.end();

    // ==================================================== page 2: overlays
    let mut p2 = layout::column();
    let mut r = layout::row();
    let popup_btn = Button::new("Popup menu").bordered();
    r.fit(&*popup_btn);
    let menu_btn = Button::new("Omarchy menu").bordered();
    r.fit(&*menu_btn);
    let confirm_btn = Button::new("Confirm dialog").bordered();
    r.fit(&*confirm_btn);
    let card_btn = Button::new("Popup card").bordered();
    r.fit(&*card_btn);
    r.close_fitted();
    let mut r = layout::row();
    let notify_btn = Button::new("Notification").bordered();
    r.fit(&*notify_btn);
    let copy_btn = Button::icon(glyph::PLAY, "Start copy (toast)").bordered();
    r.fit(&*copy_btn);
    let indet_btn = Button::new("Indeterminate toast").bordered();
    r.fit(&*indet_btn);
    let help_btn = Button::new("Shortcut help (F1)").bordered();
    r.fit(&*help_btn);
    r.close_fitted();
    p2.fit(&*Label::new("Header: gear = Preferences (Ctrl+,), bolt = Actions (F10). Drag the title to move; bottom corners to resize.").tone(Tone::Dim));
    layout::spacer();
    p2.end();

    // ==================================================== page 3: graphics
    let mut p3 = layout::column();
    let shapes = View::new(560, 140).paint(|c, u| {
        c.fill(&Path::rounded_rect(10.0, 10.0, 120.0, 110.0, [28.0, 4.0, 28.0, 4.0]), u.accent);
        c.fill(&Path::squircle(150.0, 10.0, 110.0, 110.0, 4.5), Paint::radial((205.0, 50.0), 80.0, &[(0.0, u.accent.mix(u.foreground, 0.4)), (1.0, u.accent)]));
        let card = Path::rounded_rect(290.0, 20.0, 140.0, 90.0, [14.0; 4]);
        c.shadow(&card, 0.0, 8.0, 12, gfx::Color::rgba(0.0, 0.0, 0.0, 0.6));
        c.fill(&card, u.background.mix(u.foreground, 0.12));
        c.stroke(&Path::new().move_to(450.0, 110.0).cubic_to(480.0, 10.0, 520.0, 140.0, 555.0, 30.0), &Stroke::new(4.0), u.accent);
    });
    p3.fit(&*shapes);
    let mut r = layout::row();
    let anim = View::new(200, 150).shader(gfx::shaders::accent_glow()).animate(Rate::Display);
    r.fit(&*anim);
    let noise = View::new(200, 150).shader(gfx::shaders::noise()).animate(Rate::Display);
    r.fit(&*noise);
    let star_pts: Vec<gfx::Point> = (0..10).map(|i| {
        let a = i as f32 * std::f32::consts::PI / 5.0 - std::f32::consts::FRAC_PI_2;
        let rr = if i % 2 == 0 { 60.0 } else { 26.0 };
        (75.0 + rr * a.cos(), 70.0 + rr * a.sin())
    }).collect();
    let star = Path::polygon(&star_pts);
    let s2 = star.clone();
    let shaped = View::new(150, 140).paint(move |c, u| c.fill(&s2, u.accent)).hit_shape(Hit::Fill(star))
        .on_click(|| log("star (shaped view) clicked"));
    r.fit(&*shaped);
    name(&names, &*shaped, "star");
    r.close_fitted();
    let backend = Label::new("");
    p3.fit(&*backend);
    layout::spacer();
    p3.end();

    // ==================================================== extra pages
    let mut px = layout::column();
    let extra = Label::new("");
    px.fit(&*extra);
    layout::spacer();
    px.end();
    win.end();

    // Pages: show the selected one.
    let pages = [p0.clone(), p1.clone(), p2.clone(), p3.clone(), px.clone()];
    let body2 = body.clone();
    let (ex, an) = (extra.clone(), anim.clone());
    let mut bk = backend.clone();
    page.watch(move |p| {
        log(format!("page → {} ({})", p, page_names.get(*p).unwrap_or(&"?")));
        for (i, pg) in pages.iter().enumerate() {
            let mut pg = pg.clone();
            if i == (*p).min(4) { pg.show() } else { pg.hide() }
        }
        let mut e = ex.clone();
        e.set_text(&format!("This is page {} ({}).", p, page_names.get(*p).unwrap_or(&"?")));
        bk.set_text(&format!("Shader backend: {:?} {}", an.active_backend(), an.backend_reason()));
        let mut b = body2.clone();
        b.layout();
        b.redraw();
    }).forever();

    // ---- header menus and overlays
    let w = win.clone();
    let gear = win.header().action(glyph::SETTINGS, "Preferences (Ctrl+,)", || {});
    let bolt = win.header().action("\u{f0241}", "Actions (F10)", || {});
    name(&names, &*gear, "header gear");
    name(&names, &*bolt, "header bolt");
    let g2 = gear.clone();
    let open_prefs = Rc::new(move || {
        let mut w = w.clone();
        let c = theme::corners();
        PopupMenu::open(&*g2, vec![
            PopupItem::new("", "Window: normal"),
            PopupItem::new("", "Window: floating"),
            PopupItem::new("", "Window: fullscreen").shortcut("F11").divider(),
            PopupItem::new("", "Corners: follow Omarchy").checked(c == Corners::Omarchy),
            PopupItem::new("", "Corners: square").checked(c == Corners::Square),
            PopupItem::new("", "Corners: rounded").checked(c == Corners::Rounded).divider(),
            PopupItem::new("", "Transitions on").checked(fltk_omarchy::anim::enabled()),
        ], move |i| {
            log(format!("preferences pick {i}"));
            match i {
                0 => w.set_mode(Mode::Normal),
                1 => w.set_mode(Mode::Floating),
                2 => w.set_mode(Mode::Fullscreen),
                3 => theme::set_corners(Corners::Omarchy),
                4 => theme::set_corners(Corners::Square),
                5 => theme::set_corners(Corners::Rounded),
                _ => fltk_omarchy::anim::set_enabled(!fltk_omarchy::anim::enabled()),
            }
        }).on_dismiss(|d| log(format!("preferences dismissed: {d:?}")));
    });
    let b2 = bolt.clone();
    let open_actions = Rc::new(move || {
        let items: Vec<PopupItem> = (1..=14).map(|i| PopupItem::new(glyph::FILE, &format!("Action {i}")).shortcut(&format!("Ctrl+{i}"))).collect();
        PopupMenu::open(&*b2, items, |i| log(format!("actions pick {i}"))).on_dismiss(|d| log(format!("actions dismissed: {d:?}")));
    });
    let (p1o, a1o) = (open_prefs.clone(), open_actions.clone());
    gear.clone().on_click(move || p1o());
    bolt.clone().on_click(move || a1o());
    let (p2o, a2o) = (open_prefs.clone(), open_actions.clone());
    win.shortcut("Ctrl+,", move || p2o());
    win.shortcut("F10", move || a2o());
    let w3 = win.clone();
    let open_help = Rc::new(move || {
        let row = |k: &str, d: &str| (k.to_string(), d.to_string());
        ShortcutHelp::open(&*w3, "Keyboard shortcuts", vec![
            ("Move".into(), vec![row("Tab", "Next control"), row("Shift+Tab", "Previous control"), row("Ctrl+Tab", "Next tab"), row("Ctrl+Shift+Tab", "Previous tab")]),
            ("Window".into(), vec![row("Ctrl+,", "Preferences"), row("F10", "Actions"), row("F1", "This help"), row("Ctrl+N", "Primary button")]),
            ("Lists".into(), vec![row("Up/Down", "Move"), row("Shift+Down", "Extend selection"), row("Ctrl+Space", "Toggle"), row("Ctrl+A", "Select all"), row("Enter", "Open")]),
            ("Tree".into(), vec![row("Right", "Expand / first child"), row("Left", "Collapse / parent")]),
        ]);
        log("shortcut help opened");
    });
    let h1 = open_help.clone();
    win.shortcut("F1", move || h1());
    let h2 = open_help.clone();
    help_btn.clone().on_click(move || h2());
    let pb = popup_btn.clone();
    popup_btn.clone().on_click(move || {
        PopupMenu::open(&*pb, vec![PopupItem::new(glyph::FOLDER, "Open").shortcut("Enter"), PopupItem::new(glyph::FILE, "Copy").shortcut("Ctrl+C"),
            PopupItem::new(glyph::TRASH, "Delete").disabled()], |i| log(format!("popup pick {i}")))
            .on_dismiss(|d| log(format!("popup dismissed: {d:?}")));
    });
    let w5 = win.clone();
    menu_btn.clone().on_click(move || {
        Menu::open(&*w5, "Go", vec![MenuItem::new(glyph::SEARCH, "Apps").detail("launch an app"), MenuItem::new(glyph::MONITOR, "Screenshot").detail("capture"),
            MenuItem::new(glyph::SETTINGS, "Setup").submenu()], |i| log(format!("menu pick {i}")));
        log("omarchy menu opened");
    });
    let w6 = win.clone();
    confirm_btn.clone().on_click(move || ConfirmDialog::open(&*w6, "Delete 3 files?", "Delete", true, |ok| log(format!("confirm → {ok}"))));
    let cb = card_btn.clone();
    let card_vol = vol.clone();
    card_btn.clone().on_click(move || {
        let v = card_vol.clone();
        PopupCard::open(&*cb, 280, 130, move |col| {
            col.fit(&*Label::section("Popup card"));
            let _s = Slider::new(&v, 0.0, 1.0, 0.05);
            let b = Button::new("Mute").bordered().on_click(|| log("card mute clicked"));
            col.fit(&*b);
        }).on_dismiss(|d| log(format!("card dismissed: {d:?}")));
    });
    let w7 = win.clone();
    notify_btn.clone().on_click(move || {
        Notification::show(&*w7, glyph::BELL, "Copied 3 files", "to ~/Work", Urgency::Normal);
        log("notification shown");
    });
    let w8 = win.clone();
    let start_copy = move |indeterminate: bool| {
        let Some(mut toast) = OperationToast::show(&*w8, "Copying 3 280 files") else { return };
        toast = toast.labels("Pause", "Resume", "Cancel", "Paused");
        toast.set_indeterminate(indeterminate);
        let paused = Arc::new(AtomicBool::new(false));
        let p2 = paused.clone();
        let handle = Task::new(move |ctx| {
            let mut i = 0;
            while i < 400 {
                ctx.sleep(std::time::Duration::from_millis(25))?;
                if !p2.load(Ordering::Relaxed) { i += 1; ctx.progress(i as f64 / 400.0); }
            }
            Ok(())
        });
        let (mut t1, t2) = (toast.clone(), toast.clone());
        let h = handle.on_progress(move |p| { t1.set_progress(p); t1.set_detail(&format!("{} of 3 280", (p * 3280.0) as u32)); })
            .on_done(move |o: Outcome<()>| { log(format!("copy done: {o:?}")); t2.close(); })
            .start();
        let _ = toast.on_pause(move |p| { paused.store(p, Ordering::Relaxed); log(format!("toast pause → {p}")); })
            .on_cancel(move || { log("toast cancel"); h.cancel(); });
        log(format!("copy started (indeterminate {indeterminate})"));
    };
    let sc = start_copy.clone();
    copy_btn.clone().on_click(move || sc(false));
    indet_btn.clone().on_click(move || start_copy(true));

    for (w, n) in [(&*popup_btn, "popup btn"), (&*menu_btn, "menu btn"), (&*confirm_btn, "confirm btn"), (&*card_btn, "card btn"),
                   (&*notify_btn, "notify btn"), (&*copy_btn, "copy btn"), (&*indet_btn, "indeterminate btn"), (&*help_btn, "help btn")] {
        name(&names, w, n);
    }

    win.show();
    log("window shown");

    // ---- watchers: focus, keyboard mode, window geometry, native gestures
    let last: Rc<RefCell<(String, bool, String, (u32, u32))>> = Rc::default();
    let (nm, wl) = (names.clone(), win.clone());
    fltk::app::add_timeout3(0.1, move |h| {
        let f = focus::current().map(|w| {
            let p = w.as_widget_ptr() as usize;
            nm.borrow().iter().find(|(q, _)| *q == p).map(|(_, n)| n.clone()).unwrap_or_else(|| format!("widget@{p:x}"))
        }).unwrap_or_else(|| "none".into());
        let panel = focus::focused_panel().map(|p| format!(" (panel {})", p.name())).unwrap_or_default();
        let f = f + &panel;
        let km = focus::keyboard_mode();
        let geo = hyprland::own_clients(Some("fltk-omarchy playground")).first()
            .map(|c| format!("at {:?} size {:?} floating {}", c.at, c.size, c.floating)).unwrap_or_else(|| format!("fltk {}x{}", wl.w(), wl.h()));
        let g = fltk_omarchy::native::gesture_counts();
        let mut l = last.borrow_mut();
        if l.0 != f { log(format!("focus → {f}")); l.0 = f; }
        if l.1 != km { log(format!("keyboard mode → {km}")); l.1 = km; }
        if l.2 != geo { log(format!("window {geo}")); l.2 = geo; }
        if l.3 != g { log(format!("native gestures: moves {} resizes {}", g.0, g.1)); l.3 = g; }
        fltk::app::repeat_timeout3(0.1, h);
    });
    app.run();
    log("playground closed");
}
