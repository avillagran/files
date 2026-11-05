//! Rich text: a chat transcript with Markdown, streamed replies, links,
//! code, tables and selection.
//!   cargo run --example richtext
//!   cargo run --example richtext -- --snapshot build/shots/richtext.png
//!   cargo run --example richtext -- --stream     # streams a reply from a worker thread
use fltk_omarchy::layout::FlexExt;
use fltk_omarchy::text::{Block, ColorRole, RichLabel, RichText, Style};
use fltk_omarchy::widgets::glyph;
use fltk_omarchy::{App, AppWindow};
use std::time::Duration;

pub const SAMPLE: &str = r#"## Streaming

Here is how the **streaming parser** works, with `MarkdownStream::push`:

1. Text is appended to the *open tail*.
2. Only the tail is parsed again.
   - [x] committed blocks never change
   - [ ] the last block may still grow
3. See [the docs](https://github.com/avillagran/lib-fltk-omarchy) or www.omarchy.org.

> [!TIP]
> Keep `stick_to_bottom(true)` on for chat views.

```rust
fn main() {
    let view = RichText::new().stick_to_bottom(true);
    view.push_markdown("tokens…"); // O(tail)
}
```

| Case | CPU | PSS |
|:-----|----:|----:|
| idle | 0 % | 15 MB |
| stream | low | ~+1× text |

Mixed scripts: 日本語のテキストも折り返します。 Arabic: مرحبا بالعالم. ~~Old~~ new.

---
"#;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let snapshot = args
        .iter()
        .skip_while(|a| *a != "--snapshot")
        .nth(1)
        .cloned();
    let stream = args.iter().any(|a| a == "--stream");
    let app = App::new("dev.omarchy.fltk.richtext");
    fltk_omarchy::anim::set_enabled(snapshot.is_none());
    let mut win = AppWindow::new("Rich text", 620, 760);
    let mut body = win.body();
    let title =
        RichLabel::new("Transcript of **o3** — press `F1` for keys, see [help](app://help)")
            .on_link(|u| println!("label link {u}"));
    body.fit(&*title);
    let view = RichText::new()
        .stick_to_bottom(true)
        .on_link(|url| println!("open {url}"));
    let _ = &view;
    win.header().action(glyph::CHECK, "Select all (Ctrl+A)", {
        let v = view.clone();
        move || v.select_all()
    });
    win.end();

    let who = |name: &str, role: ColorRole| {
        Block::paragraph("").push(name, &Style::plain().bold().color(role))
    };
    view.append_block(who("You", ColorRole::Accent));
    view.append_markdown("How does the rich text view handle **streaming** replies?");
    view.append_block(who("Agent", ColorRole::Green));
    view.append_markdown(SAMPLE);

    if let Some(path) = snapshot {
        win.snapshot(&path).expect("snapshot");
        println!("snapshot written to {path}");
        return;
    }
    if stream {
        view.append_block(who("Agent", ColorRole::Green));
        view.append_markdown("");
        let tx = view.sender();
        std::thread::spawn(move || {
            for _ in 0..20 {
                for word in SAMPLE.split_inclusive(' ') {
                    tx.push(word);
                    std::thread::sleep(Duration::from_millis(15));
                }
            }
            tx.finish();
        });
    }
    win.show();
    app.run();
}
