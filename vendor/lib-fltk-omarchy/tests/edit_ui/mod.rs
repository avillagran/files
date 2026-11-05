//! UI checks of `edit::TextArea` and `edit::Composer` (keys through
//! `focus::press` / `focus::type_text`, pixels from offscreen snapshots).
use super::{check, pixel};
use fltk::prelude::*;
use fltk_omarchy::edit::{Attachment, CompletionItem, Composer, TabKey, TextArea, Trigger};
use fltk_omarchy::focus::{self, press, type_text};
use fltk_omarchy::layout::FlexExt;
use fltk_omarchy::{theme, App, AppWindow, State};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

type Check<'a> = &'a dyn Fn(&Path) -> Result<(), String>;

/// Word moves and deletes: Alt on macOS, Ctrl elsewhere.
fn word_mod() -> &'static str {
    if fltk_omarchy::platform::OS == fltk_omarchy::platform::Os::MacOs { "Alt" } else { "Ctrl" }
}

fn keys(s: &str) {
    for c in s.chars() {
        match c {
            ' ' => press("Space"),
            '\n' => press("Enter"),
            c if c.is_ascii_uppercase() => press(&format!("Shift+{}", c.to_ascii_lowercase())),
            c if c.is_ascii() => press(&c.to_string()),
            c => type_text(&c.to_string()),
        };
    }
}

fn near(a: [u8; 3], b: theme::Rgb, tol: i32) -> bool {
    let b = [b.r(), b.g(), b.b()];
    a.iter()
        .zip(b.iter())
        .all(|(x, y)| (*x as i32 - *y as i32).abs() <= tol)
}

fn delete(win: AppWindow) {
    fltk::window::DoubleWindow::delete((*win).clone());
}

pub fn checks(run: &mut dyn FnMut(&str, Check)) {
    run(
        "text area: typing, Enter, word undo, grapheme Backspace through the key path",
        &|shots| {
            let mut win = AppWindow::new("t", 420, 260);
            let mut body = win.body();
            let s = State::new(String::new());
            let a = TextArea::new().placeholder("Notes").bind(&s);
            body.fit(&*a);
            win.end();
            win.snapshot(shots.join("area-empty.png"))
                .map_err(|e| e.to_string())?;
            focus::first(&*win);
            check(focus::is_focused(&*a), "focused by the router")?;
            keys("hello world\nsecond");
            check(
                s.get() == "hello world\nsecond",
                &format!("typed into the State: {:?}", s.get()),
            )?;
            press("Mod+z");
            check(
                s.get() == "hello world\n",
                &format!("undo removes the last word: {:?}", s.get()),
            )?;
            press("Mod+z");
            check(
                s.get() == "hello world",
                &format!("then the line break: {:?}", s.get()),
            )?;
            press("Mod+Shift+z");
            press("Mod+Shift+z");
            check(s.get() == "hello world\nsecond", "redo")?;
            // Committed input-method text (CJK, emoji) arrives through Fl::compose.
            type_text("日本");
            type_text("👨\u{200d}👩\u{200d}👧");
            type_text("e\u{301}");
            check(
                s.get()
                    .ends_with("second日本👨\u{200d}👩\u{200d}👧e\u{301}"),
                &format!("UTF-8 typed: {:?}", s.get()),
            )?;
            press("BackSpace");
            press("BackSpace");
            check(
                s.get().ends_with("second日本"),
                &format!("Backspace removes whole graphemes: {:?}", s.get()),
            )?;
            press("Left");
            press("Shift+Left");
            check(
                a.selected_text() == "日",
                &format!("Shift+Left selects one grapheme: {:?}", a.selected_text()),
            )?;
            // State → area.
            s.set("from the app".into());
            check(
                a.value() == "from the app" && a.caret() == 12,
                "State replaces the text, caret at the end",
            )?;
            press(&format!("{}+BackSpace", word_mod()));
            check(
                s.get() == "from the ",
                &format!("Ctrl+Backspace deletes a word: {:?}", s.get()),
            )?;
            press("Home");
            press(&format!("{}+Right", word_mod()));
            press("Shift+End");
            check(
                a.selected_text() == " the ",
                &format!("word jump + Shift+End: {:?}", a.selected_text()),
            )?;
            press("Escape");
            check(a.selection().is_empty(), "Esc clears the selection")?;
            delete(win);
            Ok(())
        },
    );

    run(
        "text area: caret and selection pixels, placeholder, focus ring colors",
        &|shots| {
            let mut win = AppWindow::new("t", 420, 200);
            let mut body = win.body();
            let a = TextArea::new().placeholder("Write here");
            body.fit(&*a);
            win.end();
            win.snapshot(shots.join("edit-layout.png"))
                .map_err(|e| e.to_string())?;
            focus::first(&*win);
            keys("WWWW");
            let p = shots.join("area-caret.png");
            win.snapshot(&p).map_err(|e| e.to_string())?;
            let (cx, cy, _, ch) = a.caret_rect();
            let (fg, focus_fill) = theme::with(|t| {
                (
                    t.palette.foreground,
                    t.fill(&t.controls.focus, t.palette.background),
                )
            });
            let px = pixel(&p, cx - 1, cy + ch / 2)?;
            check(
                near(px, fg, 40),
                &format!(
                    "caret pixel {px:?} at {},{} is the foreground",
                    cx - 1,
                    cy + ch / 2
                ),
            )?;
            press("Shift+Home");
            let p = shots.join("area-selection.png");
            win.snapshot(&p).map_err(|e| e.to_string())?;
            let (x0, y0, w0, h0) = a.text_rect("WWWW").ok_or("text rect")?;
            let sel = theme::with(|t| {
                t.palette
                    .foreground
                    .over(focus_fill, t.controls.selection_fill_alpha)
            });
            // Between two glyphs, near the top of the line: only the selection fill.
            let px = pixel(&p, x0 + w0 - 1, y0 + 1)?;
            check(
                near(px, sel, 10),
                &format!(
                    "selection {px:?} vs {:?} (rect {x0},{y0},{w0},{h0})",
                    [sel.r(), sel.g(), sel.b()]
                ),
            )?;
            let outside = pixel(&p, x0 + w0 + 30, y0 + 1)?;
            check(
                near(outside, focus_fill, 6),
                &format!("unselected background {outside:?}"),
            )?;
            delete(win);
            Ok(())
        },
    );

    run(
        "text area: auto-grows from min to max lines, then scrolls with the caret in view",
        &|shots| {
            let mut win = AppWindow::new("t", 420, 400);
            let mut body = win.body();
            let a = TextArea::new().lines(1, 3);
            body.fit(&*a);
            win.end();
            win.snapshot(shots.join("edit-layout.png"))
                .map_err(|e| e.to_string())?;
            focus::first(&*win);
            let h1 = a.h();
            keys("one\ntwo");
            let h2 = a.h();
            keys("\nthree\nfour\nfive");
            let h3 = a.h();
            win.snapshot(shots.join("area-grow.png"))
                .map_err(|e| e.to_string())?;
            check(h2 > h1 && h3 > h2, &format!("grows {h1} → {h2} → {h3}"))?;
            let lh = (h2 - h1) as f64;
            check(
                ((h3 - h1) as f64 / lh - 2.0).abs() < 0.01,
                &format!("stops at 3 lines ({h1}, {h2}, {h3})"),
            )?;
            let (_, cy, _, ch) = a.caret_rect();
            check(
                cy >= a.y() && cy + ch <= a.y() + a.h(),
                "caret line scrolled into view",
            )?;
            check(
                a.caret_on_last_line() && !a.caret_on_first_line(),
                "caret line queries",
            )?;
            press("Mod+Home");
            check(a.caret() == 0 && a.caret_on_first_line(), "Mod+Home")?;
            press("Down");
            press("Down");
            check(
                a.caret() == 8,
                &format!("Down keeps the column: {}", a.caret()),
            )?;
            press("PageDown");
            check(a.caret_on_last_line(), "PageDown reaches the end")?;
            // Long wrapped line: one paragraph, several visual lines.
            a.reset(&"word ".repeat(60));
            check(
                a.line_count() > 3,
                &format!("wraps into {} lines", a.line_count()),
            )?;
            delete(win);
            Ok(())
        },
    );

    run(
        "text area: Tab leaves, Ctrl+Tab inserts when configured; read-only; clipboard",
        &|shots| {
            let mut win = AppWindow::new("t", 420, 300);
            let mut body = win.body();
            let a = TextArea::new();
            let b = TextArea::new().tab_key(TabKey::CtrlTabInserts);
            body.fit(&*a);
            body.fit(&*b);
            win.end();
            win.snapshot(shots.join("edit-layout.png"))
                .map_err(|e| e.to_string())?;
            focus::first(&*win);
            check(focus::is_focused(&*a), "first area")?;
            press("Tab");
            check(
                focus::is_focused(&*b) && a.value().is_empty(),
                "Tab moved focus, inserted nothing",
            )?;
            press("Ctrl+Tab");
            check(
                b.value() == "\t" && focus::is_focused(&*b),
                "Ctrl+Tab inserted a tab",
            )?;
            press("Shift+Tab");
            check(focus::is_focused(&*a), "Shift+Tab back")?;
            keys("copy me");
            press("Mod+a");
            press("Mod+c");
            press("End");
            press("Mod+v");
            App::pump();
            check(
                a.value() == "copy mecopy me",
                &format!("copy + paste: {:?}", a.value()),
            )?;
            press("Mod+a");
            press("Mod+x");
            App::pump();
            check(a.value().is_empty(), "cut")?;
            let r = TextArea::new().read_only(true);
            r.set_value("fixed");
            check(r.value() == "fixed", "read-only takes values from the app")?;
            delete(win);
            Ok(())
        },
    );

    run(
        "composer: Enter submits, Shift+Enter is a new line, busy shows stop",
        &|shots| {
            let mut win = AppWindow::new("t", 520, 300);
            let mut body = win.body();
            let sent = Rc::new(RefCell::new(Vec::<String>::new()));
            let stops = Rc::new(RefCell::new(0));
            let busy = State::new(false);
            let draft = State::new(String::new());
            let (s2, st2) = (sent.clone(), stops.clone());
            let c = Composer::new()
                .bind(&draft)
                .busy(&busy)
                .on_submit(move |t| s2.borrow_mut().push(t))
                .on_stop(move || *st2.borrow_mut() += 1);
            body.fit(&*c);
            win.end();
            win.snapshot(shots.join("edit-layout.png"))
                .map_err(|e| e.to_string())?;
            focus::first(&*win);
            check(focus::is_focused(&*c.area()), "the input takes focus first")?;
            let h1 = c.h();
            keys("line one");
            press("Shift+Enter");
            keys("two");
            check(
                draft.get() == "line one\ntwo" && sent.borrow().is_empty(),
                &format!("Shift+Enter: {:?}", draft.get()),
            )?;
            check(c.h() > h1, "the composer grew with the second line")?;
            press("Enter");
            check(
                sent.borrow().as_slice() == ["line one\ntwo"],
                &format!("Enter sent: {:?}", sent.borrow()),
            )?;
            check(
                draft.get().is_empty() && c.h() == h1,
                "cleared and shrunk back",
            )?;
            press("Enter");
            check(sent.borrow().len() == 1, "empty drafts are not sent")?;
            press("Mod+z");
            check(
                draft.get() == "line one\ntwo",
                "a sent draft comes back with undo",
            )?;
            draft.set(String::new());
            busy.set(true);
            win.snapshot(shots.join("composer-busy.png"))
                .map_err(|e| e.to_string())?;
            check(
                c.shows_stop() && !c.send_button().visible(),
                "busy and empty: stop button",
            )?;
            c.stop_button().click();
            check(*stops.borrow() == 1, "stop runs on_stop")?;
            keys("follow-up");
            check(!c.shows_stop(), "a draft while busy shows send again")?;
            busy.set(false);
            delete(win);
            Ok(())
        },
    );

    run(
        "composer: completion list at the caret, Down + Enter inserts a chip, Esc closes",
        &|shots| {
            let mut win = AppWindow::new("t", 520, 420);
            let mut body = win.body();
            let queries = Rc::new(RefCell::new(Vec::<String>::new()));
            let q2 = queries.clone();
            let sent = Rc::new(RefCell::new(String::new()));
            let s2 = sent.clone();
            let c = Composer::new()
                .trigger('@', Trigger::WordStart, move |q, reply| {
                    q2.borrow_mut().push(q.text.clone());
                    let all = ["src/main.rs", "src/lib.rs", "Cargo.toml"];
                    reply.set(
                        all.iter()
                            .filter(|f| f.contains(q.text.as_str()))
                            .map(|f| CompletionItem::new(f).chip(&format!("/repo/{f}")))
                            .collect(),
                    );
                })
                .trigger('/', Trigger::LineStart, |_, reply| {
                    reply.set(vec![CompletionItem::new("review").detail("Review the diff")])
                })
                .on_submit(move |t| *s2.borrow_mut() = t);
            body.fit(&*c);
            win.end();
            win.snapshot(shots.join("edit-layout.png"))
                .map_err(|e| e.to_string())?;
            focus::first(&*win);
            keys("look at @sr");
            check(c.completion_open(), "the list opened")?;
            let (labels, cur, rect) = c.completion_state().ok_or("state")?;
            check(
                labels == ["src/main.rs", "src/lib.rs"] && cur == 0,
                &format!("filtered items {labels:?}"),
            )?;
            let p = shots.join("composer-completion.png");
            win.snapshot(&p).map_err(|e| e.to_string())?;
            let at = c.area().caret_rect_at(Some("look at ".len()));
            check(
                rect.1 >= at.1 + at.3 && rect.1 - (at.1 + at.3) < 10,
                &format!("below the caret line: popup {rect:?}, caret {at:?}"),
            )?;
            check(
                (rect.0 - at.0).abs() < 20,
                &format!("x at the trigger: {rect:?} vs {at:?}"),
            )?;
            let popup_fill = theme::with(|t| t.popups.fill(t.palette.background));
            let px = pixel(&p, rect.0 + rect.2 - 8, rect.1 + rect.3 - 5)?;
            check(near(px, popup_fill, 8), &format!("popup surface {px:?}"))?;
            press("Down");
            check(c.completion_state().map(|s| s.1) == Some(1), "Down moves")?;
            press("Enter");
            let v = c.value();
            check(
                v == "look at @src/lib.rs " && !c.completion_open(),
                &format!("inserted: {v:?}"),
            )?;
            let chips = c.area().chips();
            check(
                chips.len() == 1
                    && chips[0].data == "/repo/src/lib.rs"
                    && chips[0].range == (8..19),
                &format!("chip {chips:?}"),
            )?;
            check(sent.borrow().is_empty(), "Enter picked, did not send")?;
            // Backspace twice: the space, then the whole chip.
            press("BackSpace");
            press("BackSpace");
            check(
                c.value() == "look at " && c.area().chips().is_empty(),
                &format!("atomic chip delete: {:?}", c.value()),
            )?;
            press("Mod+z");
            check(
                c.value() == "look at @src/lib.rs " && c.area().chips().len() == 1,
                &format!(
                    "undo restores the Backspace run with the chip: {:?}",
                    c.value()
                ),
            )?;
            check(
                c.expanded_text(|_, d| d.to_string()) == "look at /repo/src/lib.rs ",
                "expanded text",
            )?;
            // Esc closes and stays closed for the same text; typing reopens.
            press("End");
            keys("@Car");
            check(c.completion_open(), "reopened")?;
            press("Escape");
            check(!c.completion_open(), "Esc closed it")?;
            keys("g");
            check(c.completion_open(), "typing reopens")?;
            press("Tab");
            check(
                c.value().ends_with("@Cargo.toml ") && focus::is_focused(&*c.area()),
                &format!("Tab accepts and keeps focus: {:?}", c.value()),
            )?;
            // Line-start trigger.
            press("Shift+Enter");
            keys("/");
            check(
                c.completion_state().is_some_and(|s| s.0 == ["review"]),
                "/ at a line start",
            )?;
            press("Escape");
            keys(" x /");
            check(!c.completion_open(), "/ inside a line does not trigger")?;
            press("Enter");
            check(
                sent.borrow().starts_with("look at @src/lib.rs @Cargo.toml"),
                &format!("sent {:?}", sent.borrow()),
            )?;
            delete(win);
            Ok(())
        },
    );

    run(
        "composer: async completions, stale answers ignored; files from paste; attachment chips",
        &|shots| {
            let mut win = AppWindow::new("t", 520, 320);
            let mut body = win.body();
            let pending: Rc<RefCell<Vec<(String, fltk_omarchy::edit::Completions)>>> =
                Rc::default();
            let p2 = pending.clone();
            let files = Rc::new(RefCell::new(Vec::<PathBuf>::new()));
            let f2 = files.clone();
            let removed = Rc::new(RefCell::new(Vec::<String>::new()));
            let r2 = removed.clone();
            let c = Composer::new()
                .trigger('$', Trigger::WordStart, move |q, reply| {
                    reply.loading();
                    p2.borrow_mut().push((q.text.clone(), reply));
                })
                .on_files(move |p| f2.borrow_mut().extend(p))
                .on_remove_attachment(move |a| r2.borrow_mut().push(a.id));
            body.fit(&*c);
            win.end();
            win.snapshot(shots.join("edit-layout.png"))
                .map_err(|e| e.to_string())?;
            focus::first(&*win);
            keys("$re");
            check(
                c.completion_open() && c.completion_state().is_some_and(|s| s.0.is_empty()),
                "Searching… while loading",
            )?;
            let n = pending.borrow().len();
            check(n == 3, &format!("one query per keystroke ({n})"))?;
            let first = pending.borrow()[n - 1].1.clone();
            let stale = pending.borrow()[0].1.clone();
            check(
                !stale.is_current() && first.is_current(),
                "only the last query is current",
            )?;
            stale.set(vec![CompletionItem::new("stale")]);
            check(
                c.completion_state().is_some_and(|s| s.0.is_empty()),
                "stale answer ignored",
            )?;
            // From a worker thread through the feed.
            let feed = first.feed();
            std::thread::spawn(move || {
                feed.set(vec![
                    CompletionItem::new("review"),
                    CompletionItem::new("refactor"),
                ])
            })
            .join()
            .unwrap();
            let end = Instant::now() + Duration::from_secs(2);
            while c.completion_state().is_some_and(|s| s.0.is_empty()) && Instant::now() < end {
                App::pump();
                std::thread::sleep(Duration::from_millis(5));
            }
            check(
                c.completion_state()
                    .is_some_and(|s| s.0 == ["review", "refactor"]),
                "answer from a thread",
            )?;
            press("Escape");
            press("Mod+a");
            press("BackSpace");
            // A file manager's clipboard: file:// URIs become files, not text.
            fltk::app::copy("file:///tmp/a%20b.png\nfile:///tmp/c.txt");
            press("Mod+v");
            App::pump();
            check(
                files.borrow().as_slice()
                    == [PathBuf::from("/tmp/a b.png"), PathBuf::from("/tmp/c.txt")],
                &format!("files {:?}", files.borrow()),
            )?;
            check(c.value().is_empty(), "no text inserted for files")?;
            let h0 = c.h();
            let atts = c.attachments();
            atts.push(Attachment::new("a", "a b.png").detail("12 KB"));
            atts.push(Attachment::new("b", "c.txt").error(true));
            check(c.h() > h0, "the attachment row appears above the input")?;
            let p = shots.join("composer-attachments.png");
            win.snapshot(&p).map_err(|e| e.to_string())?;
            // Keyboard: Shift+Tab from the input reaches the chips, Delete removes one.
            press("Shift+Tab");
            press("Right");
            press("Delete");
            check(
                removed.borrow().as_slice() == ["b"] && atts.len() == 1,
                &format!("removed {:?}", removed.borrow()),
            )?;
            press("Delete");
            check(
                atts.is_empty() && c.h() == h0 && focus::is_focused(&*c.area()),
                "last chip removed, focus back in the input",
            )?;
            delete(win);
            Ok(())
        },
    );

    run(
        "text area: 100 KB paste and typing latency stay interactive",
        &|shots| {
            let mut win = AppWindow::new("t", 640, 400);
            let mut body = win.body();
            let a = TextArea::new().lines(3, 12);
            body.fit(&*a);
            win.end();
            win.snapshot(shots.join("edit-layout.png"))
                .map_err(|e| e.to_string())?;
            focus::first(&*win);
            let para = "The quick brown fox jumps over the lazy dog, 日本語 too. ".repeat(20);
            let text: String = format!("{para}\n").repeat(100_000 / para.len() + 1);
            let t0 = Instant::now();
            a.insert(&text);
            let paste = t0.elapsed();
            let t1 = Instant::now();
            for _ in 0..50 {
                press("x");
            }
            let typing = t1.elapsed() / 50;
            // One 100 KB paragraph (worst case: every keystroke wraps it again).
            a.reset(&para.repeat(100_000 / para.len()));
            press("Mod+Home");
            let t2 = Instant::now();
            for _ in 0..20 {
                press("y");
            }
            let typing_long = t2.elapsed() / 20;
            println!("     perf: paste {} KB {:?}, typing {:?}/key, typing at the start of one {} KB paragraph {:?}/key", text.len() / 1024, paste, typing, a.value().len() / 1024, typing_long);
            check(
                paste < Duration::from_millis(1500),
                &format!("paste {paste:?}"),
            )?;
            check(
                typing < Duration::from_millis(16),
                &format!("typing {typing:?}"),
            )?;
            check(
                typing_long < Duration::from_millis(50),
                &format!("typing in a long paragraph {typing_long:?}"),
            )?;
            delete(win);
            Ok(())
        },
    );
}

#[cfg(feature = "a11y")]
pub fn a11y_checks(run: &mut dyn FnMut(&str, Check)) {
    use fltk_omarchy::a11y::testing::{self, accesskit as ak};
    run("a11y: text area value/caret, composer completion list with active descendant, send/stop names", &|shots| {
        let mut win = AppWindow::new("Chat", 520, 420);
        let mut body = win.body();
        let busy = State::new(false);
        let c = Composer::new().placeholder("Message").busy(&busy).trigger('@', Trigger::WordStart, |_, r| {
            r.set(vec![CompletionItem::new("main.rs").detail("src"), CompletionItem::new("lib.rs")]);
        });
        body.fit(&*c);
        win.end();
        win.snapshot(shots.join("edit-a11y.png")).map_err(|e| e.to_string())?;
        let rec = testing::record(&*win);
        focus::first(&*win);
        keys("hi @");
        rec.sync();
        let snap = fltk_omarchy::a11y::snapshot(&*win).unwrap();
        check(snap.contains("MultilineTextInput \"Message\" = \"hi @\""), &format!("input node:\n{snap}"))?;
        check(snap.contains("ListBox \"Suggestions\"") && snap.contains("ListBoxOption \"main.rs\""), &format!("list:\n{snap}"))?;
        check(snap.contains("Button \"Send\""), &format!("send named:\n{snap}"))?;
        let input = rec.find(ak::Role::MultilineTextInput, "Message").ok_or("input")?;
        let node = rec.node(input).ok_or("node")?;
        let sel = node.text_selection().ok_or("selection")?;
        check(
            sel.anchor.character_index == 4 && sel.focus.character_index == 4,
            &format!("caret after \"hi @\": {sel:?}"),
        )?;
        let opt = rec.find(ak::Role::ListBoxOption, "main.rs").ok_or("option")?;
        check(node.active_descendant() == Some(opt), &format!("active descendant {:?} vs {opt:?}", node.active_descendant()))?;
        check(rec.focus() == Some(input), "focus stays on the input")?;
        press("Down");
        rec.sync();
        let lib = rec.find(ak::Role::ListBoxOption, "lib.rs").ok_or("lib")?;
        check(rec.node(input).and_then(|n| n.active_descendant()) == Some(lib), "Down moves the active descendant")?;
        rec.act(opt, ak::Action::Click, None);
        check(c.value() == "hi @main.rs ", &format!("AT click accepts: {:?}", c.value()))?;
        rec.act(input, ak::Action::SetValue, Some(ak::ActionData::Value("typed by AT".into())));
        check(c.value() == "typed by AT", "set value")?;
        c.set_value("");
        busy.set(true);
        rec.sync();
        check(rec.find(ak::Role::Button, "Stop").is_some(), "stop named")?;
        delete(win);
        Ok(())
    });
}
