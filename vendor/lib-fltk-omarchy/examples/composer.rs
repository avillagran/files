//! Chat composer: a transcript (RichText) and a Composer with `@files`
//! (chips), `$skills`, `/commands` (line start), a busy state with a stop
//! button, attachments from drops and `file://` pastes.
//!   cargo run --example composer
//!   cargo run --example composer -- --snapshot build/shots/composer.png
use fltk::prelude::*;
use fltk_omarchy::edit::{Attachment, CompletionItem, Composer, Trigger};
use fltk_omarchy::layout::FlexExt;
use fltk_omarchy::task::Task;
use fltk_omarchy::text::RichText;
use fltk_omarchy::widgets::{glyph, Button};
use fltk_omarchy::{tr, App, AppWindow, State};
use std::time::Duration;

/// Files under the current directory whose path contains `q`.
fn files(q: &str) -> Vec<CompletionItem> {
    let mut out = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(".")];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            let name = p.strip_prefix("./").unwrap_or(&p).display().to_string();
            if name.starts_with('.') || name.starts_with("target") || name.starts_with("build") {
                continue;
            }
            let dir = p.is_dir();
            if dir && out.len() < 400 {
                stack.push(p.clone());
            }
            if name.to_lowercase().contains(&q.to_lowercase()) {
                let item = CompletionItem::new(&name)
                    .icon(if dir { glyph::FOLDER } else { glyph::FILE })
                    .chip(&p.display().to_string());
                out.push(item.insert(&format!("@{name}")));
            }
        }
    }
    out.sort_by_key(|i| i.label.len());
    out.truncate(50);
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let snapshot = args
        .iter()
        .skip_while(|a| *a != "--snapshot")
        .nth(1)
        .cloned();
    let app = App::new("dev.omarchy.fltk.composer");
    fltk_omarchy::anim::set_enabled(snapshot.is_none());
    let mut win = AppWindow::new("Composer", 640, 560);
    let mut body = win.body();
    let chat = RichText::new().stick_to_bottom(true); // stretches
    chat.append_markdown("**Agent** — ask anything. Type `@` for files, `$` for skills, `/` at a line start for commands. Drop files here.");
    let draft = State::new(String::new());
    let busy = State::new(false);
    let skills = ["review", "refactor", "explain", "test-writer"];
    let commands = [
        ("/model", "Choose the model"),
        ("/clear", "Start over"),
        ("/compact", "Summarize the thread"),
    ];
    let (c2, b2) = (chat.clone(), busy.clone());
    let composer = Composer::new()
        .placeholder("Ask anything, @files/folders, $skills or /commands…")
        .bind(&draft)
        .busy(&busy)
        .trigger('@', Trigger::WordStart, |q, reply| {
            // Slow sources answer later: show "Searching…", then the items.
            reply.loading();
            let q = q.text.clone();
            Task::new(move |_| Ok(files(&q)))
                .on_done(move |r| {
                    if let fltk_omarchy::task::Outcome::Done(v) = r {
                        reply.set(v)
                    }
                })
                .start();
        })
        .trigger('$', Trigger::WordStart, move |q, reply| {
            reply.set(
                skills
                    .iter()
                    .filter(|s| s.contains(q.text.as_str()))
                    .map(|s| CompletionItem::new(s).icon(glyph::PLAY))
                    .collect(),
            );
        })
        .trigger('/', Trigger::LineStart, move |q, reply| {
            reply.set(
                commands
                    .iter()
                    .filter(|(c, _)| c[1..].starts_with(q.text.as_str()))
                    .map(|(c, d)| CompletionItem::new(&c[1..]).detail(d))
                    .collect(),
            );
        })
        .on_submit(move |text| {
            c2.append_markdown(&format!("**You** — {text}"));
            b2.set(true);
            let (c3, b3) = (c2.clone(), b2.clone());
            Task::new(|ctx| {
                ctx.sleep(Duration::from_secs(2))?;
                Ok(())
            })
            .on_done(move |_| {
                c3.append_markdown("**Agent** — done.");
                b3.set(false);
            })
            .start();
        })
        .on_stop({
            let b = busy.clone();
            move || b.set(false)
        });
    let attachments = composer.attachments();
    let a2 = attachments.clone();
    let composer = composer.on_files(move |paths| {
        for p in paths {
            a2.push(Attachment::file(&p));
        }
    });
    let a3 = attachments.clone();
    let composer = composer.toolbar(move |row| {
        row.fit(
            &*Button::icon(glyph::PLUS, "")
                .tooltip(&tr!("Attach"))
                .on_click(move || {
                    a3.push(Attachment::file(std::path::Path::new("Cargo.toml")));
                }),
        );
    });
    body.fit(&*composer);
    win.end();
    win.show();
    let _ = composer.area().take_focus();
    if let Some(path) = snapshot {
        attachments.push(Attachment::file(std::path::Path::new("Cargo.toml")));
        composer.area().type_text("Please review ");
        composer
            .area()
            .insert_chip("@src/edit/area.rs", "src/edit/area.rs");
        composer.area().type_text(" and explain $");
        App::pump();
        win.snapshot(&path).expect("snapshot");
        return;
    }
    app.run();
}
