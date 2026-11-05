//! Renders the pictures of docs/COMPONENTS.md offscreen (no window is
//! shown) with the current Omarchy theme. The code of each block is the
//! snippet shown next to its picture.
//!   cargo run --example docs_shots -- docs/components
use fltk::{group::Flex, prelude::*, window::DoubleWindow};
use fltk_omarchy::focus::press;
use fltk_omarchy::layout::{self, FlexExt};
use fltk_omarchy::edit::{Attachment, CompletionItem, Composer, TextArea, Trigger};
use fltk_omarchy::list::{List, ListRow};
use fltk_omarchy::overlay::{ConfirmDialog, Dropdown, Menu, MenuItem, Notification, Urgency};
use fltk_omarchy::popups::{OperationToast, PopupItem, PopupMenu, SearchableDropdown, ShortcutHelp};
use fltk_omarchy::panel::KeyboardPanel;
use fltk_omarchy::text::{RichLabel, RichText};
use fltk_omarchy::paint;
use std::rc::Rc;
use fltk_omarchy::widgets::{glyph, *};
use fltk_omarchy::{i18n, theme, trn, App, AppWindow, State};
use std::path::{Path, PathBuf};

/// A bare window with a padded column (the picture frame).
fn frame(w: i32, h: i32) -> (DoubleWindow, Flex) {
    let win = DoubleWindow::new(0, 0, w, h, None);
    let pad = theme::with(|t| t.spacing.panel_padding);
    let mut col = layout::column();
    col.resize(pad, pad, w - 2 * pad, h - 2 * pad);
    (win, col)
}

fn done(win: DoubleWindow, col: Flex) -> DoubleWindow {
    layout::spacer();
    col.end();
    win.end();
    // Lay out now: a Flex places its children when first drawn, and the
    // popups opened below are placed from their anchor's real geometry.
    col.layout();
    for i in 0..col.children() {
        if let Some(mut g) = col.child(i).and_then(|c| c.as_group()) {
            if let Some(f) = Flex::from_dyn_widget(&g) { f.layout(); }
            g.redraw();
        }
    }
    win
}

fn save(win: &mut DoubleWindow, dir: &Path, name: &str) {
    let p = dir.join(format!("{name}.png"));
    fltk_omarchy::window::snapshot(win, &p).expect("snapshot");
    println!("{}", p.display());
}

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "docs/components".into()));
    std::fs::create_dir_all(&dir).unwrap();
    let _app = App::without_theme_watch("dev.omarchy.fltk.docs");
    fltk_omarchy::anim::set_enabled(false); // pictures show final states
    i18n::set_locale("en"); // the toolkit's own strings, whatever the system language

    // ---- buttons
    let (win, col) = frame(600, 76);
    let mut r = layout::row();
    r.fit(&*Button::new("Default"));
    r.fit(&*Button::new("Bordered").bordered());
    r.fit(&*Button::icon(glyph::PLUS, "New").primary());
    r.fit(&*Button::icon(glyph::TRASH, "Delete").danger().bordered());
    r.fit(&*ActionButton::new(glyph::REFRESH, "Refresh"));
    r.fit(&*ActionButton::destructive(glyph::CLOSE, "Close"));
    r.close_fitted();
    save(&mut done(win, col), &dir, "buttons");

    // ---- button group
    let (win, col) = frame(600, 76);
    let period = State::new(1usize);
    let mut r = layout::row();
    r.fit(&*ButtonGroup::new(&["Day", "Week", "Month"], &period));
    r.close_fitted();
    save(&mut done(win, col), &dir, "button-group");

    // ---- text fields
    let (win, mut col) = frame(600, 176);
    let query = State::new(String::new());
    col.fit(&*TextField::new().placeholder("Search apps…").bind(&query));
    let name = State::new("Omarchy".to_string());
    col.fit(&*TextField::new().bind(&name));
    let mut r = layout::row();
    r.fit(&*TextField::password().placeholder("Password"));
    r.fit(&*NumberField::new(&State::new(42), 0, 100, 1));
    r.close_fitted();
    save(&mut done(win, col), &dir, "text-fields");

    // ---- text area and composer
    let (win, mut col) = frame(600, 330);
    let notes = State::new("Notes wrap at the edge of the field and the field grows from 2 to 6 lines, then scrolls.\nCtrl+Z undoes a word at a time.".to_string());
    col.fit(&*TextArea::new().placeholder("Notes").lines(2, 6).bind(&notes));
    let draft = State::new(String::new());
    let composer = Composer::new()
        .placeholder("Ask anything, @files or /commands…")
        .bind(&draft)
        .trigger('@', Trigger::WordStart, |q, reply| {
            let files = ["src/main.rs", "src/lib.rs", "src/edit/composer.rs", "Cargo.toml"];
            reply.set(files.iter().filter(|f| f.contains(q.text.as_str())).map(|f| CompletionItem::new(f).icon(glyph::FILE).chip(f)).collect());
        })
        .on_submit(|text| println!("send {text}"))
        .on_files(|paths| println!("attach {paths:?}"));
    composer.attachments().push(Attachment::new("shot.png", "shot.png").detail("84 KB"));
    col.fit(&*composer);
    let mut win = done(win, col);
    // picture: after typing "Review @src" (the list opens at the caret)
    let _ = composer.area().take_focus();
    composer.area().type_text("Review @src");
    save(&mut win, &dir, "composer");

    // ---- toggles
    let (win, mut col) = frame(600, 214);
    let wifi = State::new(true);
    let mut r = layout::row();
    r.fit(&*Label::new("Wi-Fi"));
    layout::spacer();
    r.fit(&*ToggleSwitch::new(&wifi));
    r.close_fitted();
    col.fit(&*Toggle::new("Night light", "Warmer colors after sunset", &State::new(true)));
    col.fit(&*Toggle::new("Do not disturb", "Silence notifications", &State::new(false)));
    save(&mut done(win, col), &dir, "toggles");

    // ---- slider and progress
    let (win, mut col) = frame(600, 110);
    let volume = State::new(0.64);
    let mut r = layout::row();
    r.fit(&*Label::new("Volume"));
    let _slider = Slider::new(&volume, 0.0, 1.0, 0.05);
    r.fit(&*Label::new("").right().bind(&volume.map(|v| format!("{:.0}%", v * 100.0))));
    r.close_fitted();
    col.fit(&*Progress::new(&State::new(0.42)));
    save(&mut done(win, col), &dir, "slider-progress");

    // ---- text styles, hero, separator
    let (win, mut col) = frame(600, 250);
    col.fit(&*Hero::new(glyph::VOLUME_HIGH, "Audio", "Built-in speakers").detail("64%"));
    col.fit(&*Separator::new());
    col.fit(&*Label::section("Output"));
    col.fit(&*Label::styled("Title text", TextStyle::Title));
    col.fit(&*Label::new("Body text"));
    col.fit(&*Label::new("Dim caption").tone(Tone::Dim));
    save(&mut done(win, col), &dir, "labels");

    // ---- rows
    let (win, mut col) = frame(600, 160);
    col.fit(&*Row::new(glyph::MONITOR, "HDMI / DisplayPort").detail("Digital"));
    col.fit(&*Row::new(glyph::HEADPHONES, "Built-in speakers").detail("Analog").current(true));
    col.fit(&*Row::new(glyph::BLUETOOTH, "WH-1000XM4").detail("Bluetooth").on_click(|| println!("connect")));
    save(&mut done(win, col), &dir, "rows");

    // ---- tabs
    let (win, mut col) = frame(600, 76);
    let tab = State::new(1usize);
    col.fit(&*TabBar::new(&["Home", "Downloads", "Work"], &tab).on_close(|i| println!("close {i}")));
    save(&mut done(win, col), &dir, "tabs");

    // ---- sidebar (collapsed, then expanded as hover or focus leaves it)
    for (name, open) in [("sidebar", false), ("sidebar-expanded", true)] {
        let (win, mut col) = frame(600, 230);
        let view = State::new(0usize);
        let mut row = layout::row();
        let side = Sidebar::new(vec![
            SidebarItem::new(glyph::FOLDER, "Files").badge("12"),
            SidebarItem::new(glyph::SEARCH, "Search"),
            SidebarItem::new(glyph::BELL, "Notifications").badge("3"),
            SidebarItem::new(glyph::SETTINGS, "Settings").footer(),
        ], &view);
        row.fit(&*side);
        let mut content = layout::column();
        content.fit(&*Label::styled("Files", TextStyle::Title));
        content.fit(&*Label::new("The content makes room when the rail expands.").tone(Tone::Dim));
        content.end();
        row.end();
        col.fixed(&row, 190);
        // picture: expanded (pointer resting on it, focus inside, or pinned)
        if open { side.expand(); }
        save(&mut done(win, col), &dir, name);
    }

    // ---- dropdown (open)
    let (win, mut col) = frame(600, 210);
    let output = State::new(0usize);
    let mut dd = Dropdown::new("Output", &["Speakers", "Headphones", "HDMI"], &output);
    col.fit(&*dd);
    let mut win = done(win, col);
    dd.open();
    save(&mut win, &dir, "dropdown");

    // ---- searchable dropdown (open, filtered)
    let (win, mut col) = frame(600, 320);
    let place = State::new(None::<usize>);
    let places = vec![
        ("Home".to_string(), "~/".to_string()),
        ("Downloads".to_string(), "~/Downloads".to_string()),
        ("Work".to_string(), "~/Work".to_string()),
        ("Pictures".to_string(), "~/Pictures".to_string()),
    ];
    let mut sd = SearchableDropdown::new("Go to…", places, &place);
    col.fit(&*sd);
    let mut win = done(win, col);
    sd.open();
    press("o");
    save(&mut win, &dir, "searchable-dropdown");

    // ---- virtual list
    let (win, mut col) = frame(600, 300);
    let count = State::new(100_000usize);
    let selected = State::new(Some(2usize));
    let list = List::new(&count, |i| ListRow::new(&format!("Item {i}")).icon(glyph::FILE).detail(&format!("{} KB", i * 3)))
        .bind_selected(&selected);
    col.fit(&*list);
    col.fixed(&*list, 250);
    save(&mut done(win, col), &dir, "list");

    // ---- rich text (Markdown, chat streaming)
    let (win, mut col) = frame(600, 400);
    col.fit(&*RichLabel::new("Agent reply, **streaming** — see [the docs](https://example.com)"));
    let chat = RichText::new().stick_to_bottom(true).on_link(|url| println!("open {url}"));
    col.fit(&*chat);
    col.fixed(&*chat, 330);
    chat.append_markdown("Use `push_markdown` to **stream** tokens into the *last* message:\n\n\
        - [x] only the open tail is parsed again\n- [ ] the view sticks to the bottom\n\n\
        ```rust\nchat.push_markdown(\"more tokens…\"); // O(tail)\n```\n\n\
        | Messages | Blocks laid out |\n|---|--:|\n| 5 000 | ~40 |\n\n> Links: https://example.com");
    chat.push_markdown(" and [docs](https://example.com/docs).");
    save(&mut done(win, col), &dir, "rich-text");

    // ---- popup menu (header Actions menu / context menu)
    let (win, col) = frame(600, 260);
    let mut r = layout::row();
    let actions = Button::icon("\u{f0241}", "Actions");
    r.fit(&*actions);
    r.close_fitted();
    let mut win = done(win, col);
    PopupMenu::open(&*actions, vec![
        PopupItem::new(glyph::FOLDER, "Open").shortcut("Enter"),
        PopupItem::new(glyph::SEARCH, "Preview").shortcut("Space").divider(),
        PopupItem::new(glyph::FILE, "Copy").shortcut("Mod+C"),
        PopupItem::new(glyph::FILE, "Show hidden files").checked(true),
        PopupItem::new(glyph::TRASH, "Move to trash").shortcut("Delete").disabled(),
    ], |i| println!("picked {i}"));
    save(&mut win, &dir, "popup-menu");

    // ---- popup menu larger than the window: clamped, scrolls, keeps the cursor visible
    let (win, col) = frame(600, 230);
    let mut r = layout::row();
    layout::spacer();
    let more = Button::icon("\u{f0241}", "Actions");
    r.fit(&*more);
    r.close_fitted();
    let mut win = done(win, col);
    let items: Vec<PopupItem> = (1..=12).map(|i| PopupItem::new(glyph::FILE, &format!("Action {i}")).shortcut(&format!("Ctrl+{i}"))).collect();
    let menu = PopupMenu::open(&*more, items, |i| println!("picked {i}"))
        .on_dismiss(|reason| println!("closed: {reason:?}"));   // Picked(i), Escape, ClickOutside, Closed
    press("End");                                              // scrolls to the last entry
    save(&mut win, &dir, "popup-menu-scroll");
    menu.close();                                              // e.g. the data changed

    // ---- popup menu with submenus (long context menus)
    let (win, col) = frame(600, 260);
    let mut r = layout::row();
    let actions = Button::icon("\u{f0241}", "Actions");
    r.fit(&*actions);
    r.close_fitted();
    let mut win = done(win, col);
    PopupMenu::open_id(&*actions, vec![
        PopupItem::new(glyph::FOLDER, "Open").shortcut("Enter").id("open").divider(),
        PopupItem::submenu(glyph::FILE, "Edit", vec![
            PopupItem::new("", "Copy").shortcut("Mod+C").id("copy"),
            PopupItem::new("", "Paste").shortcut("Mod+V").id("paste"),
            PopupItem::new("", "Rename").shortcut("F2").id("rename"),
        ]),
        PopupItem::submenu(glyph::SEARCH, "Share", vec![
            PopupItem::new("", "Email").id("share.email"),
            PopupItem::new("", "Copy link").id("share.link"),
        ]).divider(),
        PopupItem::new(glyph::TRASH, "Move to trash").shortcut("Delete").id("trash"),
    ], |id| println!("picked {id}"));
    press("Down");                                             // Edit
    press("Right");                                            // opens its submenu
    save(&mut win, &dir, "popup-menu-submenu");

    // ---- Omarchy menu (type to filter)
    let (win, col) = frame(600, 420);
    let mut win = done(win, col);
    Menu::open(&win, "Go", vec![
        MenuItem::new(glyph::SEARCH, "Apps").detail("launch an app"),
        MenuItem::new(glyph::MONITOR, "Screenshot").detail("capture the screen"),
        MenuItem::new(glyph::MONITOR, "Screen record").detail("record the screen"),
        MenuItem::new(glyph::SETTINGS, "Setup").submenu(),
    ], |i| println!("picked {i}"));
    press("s"); press("c");
    save(&mut win, &dir, "menu");

    // ---- confirm dialog
    let (win, col) = frame(600, 260);
    let mut win = done(win, col);
    ConfirmDialog::open(&win, "Move 3 files to the trash?", "Move", true, |ok| println!("confirmed: {ok}"));
    save(&mut win, &dir, "confirm-dialog");

    // ---- notifications
    let (win, col) = frame(600, 200);
    let mut win = done(win, col);
    Notification::show(&win, glyph::BELL, "Copied 3 files", "to ~/Work/reports", Urgency::Normal);
    Notification::show(&win, glyph::WIFI, "Connected", "", Urgency::Low);
    save(&mut win, &dir, "notification");

    // ---- operation toast
    let (win, col) = frame(600, 200);
    let mut win = done(win, col);
    let mut op = OperationToast::show(&win, "Copying 3 files").unwrap()
        .on_pause(|paused| println!("paused: {paused}"))
        .on_cancel(|| println!("cancel"));
    op.set_progress(0.42);
    op.set_detail("report.pdf · 12 of 30 MB");
    save(&mut win, &dir, "operation-toast");
    op.close();

    // ---- F1 shortcut help
    let (win, col) = frame(600, 420);
    let mut win = done(win, col);
    let row = |k: &str, d: &str| (k.to_string(), d.to_string());
    ShortcutHelp::open(&win, "Keyboard shortcuts", vec![
        ("Move".into(), vec![row("Tab", "Next control"), row("Shift+Tab", "Previous control"), row("Ctrl+Tab", "Next tab")]),
        ("Files".into(), vec![row("Enter", "Open"), row("F2", "Rename"), row("Delete", "Move to trash")]),
        ("Window".into(), vec![row("Mod+,", "Preferences"), row("F10", "Actions"), row("F1", "This help")]),
    ]);
    save(&mut win, &dir, "shortcut-help");

    // ---- app window: header actions + body
    let mut w = AppWindow::new("Notes", 600, 260);
    w.header().action(glyph::SETTINGS, "Preferences", || {});
    w.header().action("\u{f0241}", "Actions", || {});
    w.header().destructive_action(glyph::CLOSE, "Close", || {});
    let mut body = w.body();
    body.fit(&*TextField::new().placeholder("Title"));
    body.fit(&*Toggle::new("Pinned", "Keep at the top of the list", &State::new(true)));
    let mut r = layout::row();
    layout::spacer();
    r.fit(&*Button::new("Cancel").bordered());
    r.fit(&*Button::new("Save").primary());
    r.close_fitted();
    w.end();
    w.snapshot(dir.join("app-window.png")).unwrap();
    println!("{}", dir.join("app-window.png").display());

    // ---- tooltips: nothing to call, every tooltip is drawn by the toolkit
    let mut win = AppWindow::new("Notes", 600, 160);
    let prefs = win.header().action(glyph::SETTINGS, "Preferences", || {});
    win.header().action("\u{f0241}", "Actions", || {});
    let mut body = win.body();
    body.fit(&*Button::new("Save").bordered().tooltip("Save the note"));
    win.end();
    // picture: the pointer rests on the gear for 400 ms
    win.snapshot(dir.join("tooltip.png")).unwrap(); // lays the window out
    fltk_omarchy::tooltip::show(&*prefs);
    win.snapshot(dir.join("tooltip.png")).unwrap();
    fltk_omarchy::tooltip::hide();
    println!("{}", dir.join("tooltip.png").display());

    // ---- i18n: one window, bound to the toolkit catalog, in Arabic and Japanese
    for loc in ["ar", "ja"] {
        i18n::set_locale(loc);
        let mut w = AppWindow::new("", 460, 230);
        w.bind_title(&i18n::text("Preferences"));
        let mut body = w.body();
        body.fit(&*Label::section("").bind(&i18n::text("Language")));
        body.fit(&*TextField::new().bind_placeholder(&i18n::text("Search…")));
        let n = State::new(5);
        body.fit(&*Label::new("").bind(&i18n::map(&n, |n| trn!("{count} items", *n))));
        let mut r = layout::row();
        layout::spacer();
        r.fit(&*Button::new("").bordered().bind_text(&i18n::text("Cancel")));
        r.fit(&*Button::new("").primary().bind_text(&i18n::text("Save")));
        r.close_fitted();
        w.end();
        let p = dir.join(format!("i18n-{loc}.png"));
        w.snapshot(&p).unwrap();
        println!("{}", p.display());
    }
    i18n::set_locale("en");

    // ---- keyboard: focus ring while driving the app with the keyboard
    let mut w = AppWindow::new("Keyboard", 600, 330);
    let mut body = w.body();
    body.fit(&*TextField::new().placeholder("Search…"));
    let view = State::new(0usize);
    let mut r = layout::row();
    r.fit(&*ButtonGroup::new(&["Day", "Week", "Month"], &view));
    r.close_fitted();
    let tab = State::new(0usize);
    body.fit(&*TabBar::new(&["Home", "Downloads", "Work"], &tab));
    let _list = List::new(&State::new(1000usize), |i| ListRow::new(&format!("Item {i}")).icon(glyph::FILE));
    w.end();
    fltk_omarchy::focus::first(&*w);
    press("Tab");
    press("Right");                           // button group: Week
    w.snapshot(dir.join("keyboard-1-group.png")).unwrap();
    press("Tab"); press("Ctrl+Tab");          // tab bar: Downloads
    w.snapshot(dir.join("keyboard-2-tabs.png")).unwrap();
    press("Tab"); press("Down"); press("Down"); // list cursor
    w.snapshot(dir.join("keyboard-3-list.png")).unwrap();
    println!("{}", dir.join("keyboard-*.png").display());

    // ---- keyboard panels: two Tab stops inside one canvas (dual pane)
    let mut w = AppWindow::new("Panels", 600, 320);
    let mut body = w.body();
    body.fit(&*TextField::new().placeholder("~/Work"));
    let names: Rc<Vec<&str>> = Rc::new(vec!["docs", "src", "tests", "Cargo.toml", "README.md", "LICENSE", "build.rs", "notes.txt"]);
    let left = KeyboardPanel::new("left");
    let right = KeyboardPanel::new("right");
    for p in [&left, &right] {
        // IDs are stable keys from your model (here: 100 + position).
        p.set_items(names.len(), |i| 100 + i as u64, |id| id.checked_sub(100).map(|x| x as usize));
    }
    let mut canvas = fltk::widget::Widget::default().with_size(560, 200);
    canvas.handle(|_, ev| matches!(ev, fltk::enums::Event::Focus | fltk::enums::Event::Unfocus));
    let (l2, r2, n2) = (left.clone(), right.clone(), names.clone());
    canvas.draw(move |c| theme::with(|t| {
        let half = c.w() / 2;
        for (k, p) in [&l2, &r2].into_iter().enumerate() {
            let x = c.x() + k as i32 * half + 2;
            let bg = t.palette.background;
            for (i, name) in n2.iter().enumerate() {
                let id = 100 + i as u64;
                let y = c.y() + 6 + i as i32 * 24;
                let fill = if p.cursor() == Some(id) && p.has_focus() { Some(t.fill(&t.controls.hover, bg)) }
                    else if p.is_selected(id) { Some(t.fill(&t.controls.selected, bg)) } else { None };
                paint::surface(x + 4, y, half - 12, 22, t.radius, fill, None);
                paint::text(name, x + 14, y, half - 30, 22, t.font.body, p.is_selected(id), t.palette.foreground, fltk::enums::Align::Left);
            }
            if p.show_ring() {
                paint::stroke(x, c.y(), half - 4, c.h(), t.radius, 1, t.border(&t.controls.focus, bg));
            }
        }
    }));
    body.fit(&canvas);
    fltk_omarchy::focus::host(&canvas, &[&left, &right]);   // Tab: path → left → right
    left.on_activate(|id| println!("open {id}"));
    let r3 = right.clone();
    left.on_edge(move |k| k == fltk::enums::Key::Right && { r3.focus(); true });
    w.end();
    fltk_omarchy::focus::first(&*w);
    press("Tab"); press("Down"); press("Down");              // left: cursor on "src"
    press("Tab"); press("Down"); press("Shift+Down"); press("Shift+Down"); // right: range
    w.snapshot(dir.join("keyboard-panels.png")).unwrap();
    println!("{}", dir.join("keyboard-panels.png").display());
}
