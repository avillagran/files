//! Right-to-left layouts: the same windows in English and in Arabic,
//! switched live (no rebuild). Every row, header, list, table, tree, menu,
//! dialog and toast mirrors; the media transport row keeps its left-to-right
//! order with `direction(Ltr)`; RichText lays out mixed Arabic, Latin and
//! numbers with the Unicode bidi algorithm.
//!   cargo run --example rtl                       # live window, F2 switches en/ar
//!   cargo run --example rtl -- --locale ar        # start in Arabic
//!   cargo run --example rtl -- --snapshot build/rtl   # PNGs: <name>-en.png, <name>-ar.png
use fltk::prelude::*;
use fltk_omarchy::layout::{self, Direction, FlexExt};
use fltk_omarchy::list::{List, ListRow};
use fltk_omarchy::overlay::{ConfirmDialog, Menu, MenuItem, Notification, Urgency};
use fltk_omarchy::panel::KeyboardPanel;
use fltk_omarchy::popups::{OperationToast, PopupItem, PopupMenu};
use fltk_omarchy::table::{Column, Table, Tree, TreeNode};
use fltk_omarchy::text::RichText;
use fltk_omarchy::widgets::{glyph, *};
use fltk_omarchy::{i18n, tr, App, AppWindow, State};

const CATALOG: &str = r#"{
  "en": {},
  "ar": {
    "Files": "الملفات", "Audio": "الصوت", "Built-in speakers": "مكبرات الصوت المدمجة",
    "Night light": "الإضاءة الليلية", "Warmer colors after sunset": "ألوان أدفأ بعد الغروب",
    "Do not disturb": "عدم الإزعاج", "Silence notifications": "كتم الإشعارات",
    "Home": "الرئيسية", "Documents": "المستندات", "Pictures": "الصور", "Music": "الموسيقى",
    "Day": "يوم", "Week": "أسبوع", "Month": "شهر", "Volume": "مستوى الصوت",
    "Media controls (always left to right)": "أزرار الوسائط (دائمًا من اليسار إلى اليمين)",
    "Name": "الاسم", "Size": "الحجم", "Kind": "النوع", "Folder": "مجلد", "Text": "نص",
    "Delete 3 files permanently?": "حذف 3 ملفات نهائيًا؟", "Copying files": "نسخ الملفات",
    "Screenshot saved": "تم حفظ لقطة الشاشة", "Go": "انتقال", "Apps": "التطبيقات",
    "Style": "النمط", "System": "النظام", "Theme": "السمة", "Output": "المخرج",
    "Digital": "رقمي", "Analog": "تناظري", "Progress": "التقدم"
  }
}"#;

const MIXED: &str = "## مرحبا بالعالم\n\nهذا نص **عربي** مع كلمات English و`code` وأرقام 2026 و3.14، ورابط [Omarchy](https://omarchy.org).\n\nA left-to-right paragraph with an Arabic phrase: «السلام عليكم» in the middle.\n\n- عنصر أول مع ملف README.md\n- عنصر ثانٍ: 42 ملفًا\n\n> اقتباس: البرنامج يعمل على Linux و macOS و Windows.\n\n| الملف | الحجم |\n|---|--:|\n| notes.md | 12 KB |\n| صورة.png | 1.2 MB |\n\n```rust\nlet greeting = \"مرحبا\"; // code stays left to right\n```";

struct Shell {
    win: AppWindow,
    gear: ActionButton,
}

fn shell() -> Shell {
    let mut win = AppWindow::new("", 620, 700);
    win.bind_title(&i18n::text("Files"));
    let gear = win
        .header()
        .action(glyph::SETTINGS, "", || {})
        .bind_tooltip(&i18n::text("Preferences"));
    win.header()
        .action(glyph::SEARCH, "", || {})
        .bind_tooltip(&i18n::text("Search"));
    win.header()
        .destructive_action(glyph::CLOSE, "", || {})
        .bind_tooltip(&i18n::text("Close"));
    let mut body = win.body();
    let hero = Hero::new(glyph::VOLUME_HIGH, "", "").detail("64%");
    let h2 = hero.clone();
    i18n::bind(&*hero, move || {
        let mut h = h2.clone();
        h.set_title(&tr!("Audio"));
        h.set_meta(&tr!("Built-in speakers"));
    });
    body.fit(&*hero);
    let tabs_v = State::new(1usize);
    let tabs = TabBar::new(&["", "", "", ""], &tabs_v).on_close(|_| {});
    let t2 = tabs.clone();
    i18n::bind(&*tabs, move || {
        let names = [tr!("Home"), tr!("Documents"), tr!("Pictures"), tr!("Music")];
        t2.clone()
            .set_tabs(&names.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    });
    body.fit(&*tabs);

    let mut r = layout::row();
    r.fit(
        &*Button::icon(glyph::FOLDER, "")
            .bordered()
            .bind_text(&i18n::text("Open")),
    );
    r.fit(
        &*Button::icon(glyph::PLUS, "")
            .primary()
            .bind_text(&i18n::text("Save")),
    );
    r.fit(
        &*Button::icon(glyph::TRASH, "")
            .danger()
            .bordered()
            .bind_text(&i18n::text("Delete")),
    );
    layout::spacer();
    r.fit(&*ActionButton::new(glyph::REFRESH, "").bind_tooltip(&i18n::text("Refresh")));
    r.close_fitted();

    let period = State::new(0usize);
    let mut r = layout::row();
    r.fit(&*ButtonGroup::new(&["Day", "Week", "Month"], &period));
    layout::spacer();
    let n = State::new(42i64);
    r.fit(&*NumberField::new(&n, 0, 100, 1));
    r.close_fitted();

    body.fit(&*TextField::new().bind_placeholder(&i18n::text("Search…")));

    let vol = State::new(0.7);
    let mut r = layout::row();
    r.fit(&*Label::new("").tone(Tone::Dim).bind(&i18n::text("Volume")));
    let _slider = Slider::new(&vol, 0.0, 1.0, 0.05);
    r.close_fitted();
    let progress = State::new(0.3);
    let mut r = layout::row();
    r.fit(&*Label::new("").tone(Tone::Dim).bind(&i18n::text("Progress")));
    let _p = Progress::new(&progress);
    r.close_fitted();

    // Media transport: same order in every language.
    body.fit(&*Label::section("").bind(&i18n::text("Media controls (always left to right)")));
    let mut media = layout::row();
    media.direction(Direction::Ltr);
    media.fit(&*Button::icon(glyph::CHEVRON_LEFT, "").bordered());
    media.fit(&*Button::icon(glyph::PLAY, "").primary());
    media.fit(&*Button::icon(glyph::STOP, "").bordered());
    media.fit(&*Button::icon(glyph::CHEVRON_RIGHT, "").bordered());
    let _timeline = Slider::new(&State::new(0.25), 0.0, 1.0, 0.01);
    media.close_fitted();

    body.fit(
        &*Toggle::new("", "", &State::new(true))
            .bind_title(&i18n::text("Night light"))
            .bind_description(&i18n::text("Warmer colors after sunset")),
    );
    body.fit(
        &*Toggle::new("", "", &State::new(false))
            .bind_title(&i18n::text("Do not disturb"))
            .bind_description(&i18n::text("Silence notifications")),
    );
    let row = Row::new(glyph::MONITOR, "HDMI / DisplayPort").detail("Digital");
    body.fit(&*row);
    body.fit(
        &*Row::new(glyph::HEADPHONES, "Built-in speakers")
            .detail("Analog")
            .current(true),
    );
    win.end();
    Shell { win, gear }
}

fn data() -> AppWindow {
    let mut win = AppWindow::new("", 760, 330);
    win.bind_title(&i18n::text("Files"));
    let body = win.body();
    let mut r = fltk::group::Flex::default().row();
    r.set_spacing(12);
    let count = State::new(1000usize);
    let sel = State::new(Some(1usize));
    let _list = List::new(&count, |i| {
        ListRow::new(&format!("{} {i}", tr!("Text")))
            .icon(glyph::FILE)
            .detail(&format!("{} KB", i * 3))
    })
    .bind_selected(&sel);
    let files = [
        ("notes.md", "12 KB"),
        ("صورة.png", "1.2 MB"),
        ("README", "4 KB"),
        ("مستند.pdf", "380 KB"),
    ];
    let panel = KeyboardPanel::new("files");
    let n = files.len();
    panel.set_items(
        n,
        |i| i as u64,
        move |id| (id < n as u64).then_some(id as usize),
    );
    panel.set_cursor(Some(1));
    panel.select_only(1);
    let table = Table::new(
        &panel,
        vec![Column::new("", 0).flex(), Column::new("", 80).right()],
        move |row, col| {
            if col == 0 {
                files[row].0.to_string()
            } else {
                files[row].1.to_string()
            }
        },
    )
    .icon(|row| {
        if row == 2 {
            glyph::FOLDER.into()
        } else {
            glyph::FILE.into()
        }
    });
    let tpanel = KeyboardPanel::new("tree");
    let tree = Tree::new(&tpanel, |parent| match parent {
        None => vec![
            TreeNode::new(1, &tr!("Home")).icon(glyph::FOLDER).branch(),
            TreeNode::new(2, &tr!("Music")).icon(glyph::FOLDER).branch(),
        ],
        Some(1) => vec![
            TreeNode::new(10, &tr!("Documents"))
                .icon(glyph::FOLDER)
                .branch()
                .detail("3"),
            TreeNode::new(11, "notes.md")
                .icon(glyph::FILE)
                .detail("12 KB"),
        ],
        Some(10) => vec![TreeNode::new(100, "a.txt").icon(glyph::FILE)],
        _ => vec![],
    });
    tree.expand(1);
    tree.expand(10);
    r.end();
    let _ = (&table, &tree);
    let _ = body;
    win.end();
    // Column titles and tree labels follow the language.
    let (tb, tr2) = (table.clone(), tree.clone());
    i18n::bind(&*table, move || {
        tb.set_titles(&[&tr!("Name"), &tr!("Size")]);
        tr2.refresh();
    });
    win
}

fn main() {
    let shots = std::env::args().skip_while(|a| a != "--snapshot").nth(1);
    let app = App::new("dev.omarchy.fltk.rtl");
    i18n::load(CATALOG).expect("demo catalog");
    i18n::set_locale("en");
    let mut sh = shell();
    let mut data = data();
    let mut tw = AppWindow::new("RichText", 620, 640);
    let body = tw.body();
    let rt = RichText::new();
    rt.set_markdown(MIXED);
    let _ = body;
    tw.end();

    let Some(dir) = shots else {
        if let Some(l) = std::env::args().skip_while(|a| a != "--locale").nth(1) {
            i18n::set_locale(&l);
        }
        sh.win.shortcut("F2", || {
            i18n::set_locale(if i18n::is_rtl() { "en" } else { "ar" })
        });
        sh.win.show();
        app.run();
        return;
    };
    std::fs::create_dir_all(&dir).expect("snapshot dir");
    fltk_omarchy::anim::set_enabled(false);
    for loc in ["en", "ar"] {
        // Same windows: the locale switch re-lays them out.
        i18n::set_locale(loc);
        let p = |n: &str| format!("{dir}/{n}-{loc}.png");
        sh.win.snapshot(p("rtl-shell")).unwrap();
        data.snapshot(p("rtl-data")).unwrap();
        tw.snapshot(p("rtl-text")).unwrap();
        // Popups take the direction when they open.
        let items = vec![
            PopupItem::new(glyph::FOLDER, &tr!("Open")).shortcut("Mod+O"),
            PopupItem::new(glyph::PLUS, &tr!("Save"))
                .shortcut("Mod+S")
                .divider(),
            PopupItem::new("", &tr!("Fullscreen")).checked(true),
            PopupItem::new(glyph::TRASH, &tr!("Delete")).shortcut("Delete"),
        ];
        let h = PopupMenu::open(&*sh.gear, items, |_| {});
        sh.win.snapshot(p("rtl-menu")).unwrap();
        h.close();
        Menu::open(
            &*sh.win,
            &tr!("Go"),
            vec![
                MenuItem::new("\u{f0047}", &tr!("Apps")).submenu(),
                MenuItem::new("\u{f03d8}", &tr!("Style")).submenu(),
                MenuItem::new("\u{f0425}", &tr!("System")).submenu(),
                MenuItem::new(glyph::SETTINGS, &tr!("Preferences")),
            ],
            |_| {},
        );
        sh.win.snapshot(p("rtl-command")).unwrap();
        fltk_omarchy::focus::press("Escape");
        close_overlays(&mut sh.win);
        ConfirmDialog::open(
            &*sh.win,
            &tr!("Delete 3 files permanently?"),
            &tr!("Delete"),
            true,
            |_| {},
        );
        sh.win.snapshot(p("rtl-confirm")).unwrap();
        close_overlays(&mut sh.win);
        let mut toast = OperationToast::show(&*sh.win, &tr!("Copying files")).unwrap();
        toast.set_progress(0.4);
        toast.set_detail("notes.md → ~/Documents");
        Notification::show(
            &*sh.win,
            glyph::BELL,
            &tr!("Screenshot saved"),
            "~/Pictures/2026-11-04.png",
            Urgency::Critical,
        );
        sh.win.snapshot(p("rtl-toast")).unwrap();
        toast.close();
        close_overlays(&mut sh.win);
    }
    println!("snapshots in {dir}");
}

/// Deletes every overlay layer (children after the window's root flex).
fn close_overlays(win: &mut AppWindow) {
    while win.children() > 1 {
        let Some(c) = win.child(win.children() - 1) else {
            break;
        };
        win.remove(&c);
        fltk::app::delete_widget(c);
    }
}
