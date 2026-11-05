//! Real key events into a TextArea (driven by `script/edit-input-check`
//! with XTest inside its own Xvfb display, never the user's session):
//! dead keys and compose through XIM, keysyms of CJK and emoji, Shift+Enter,
//! Backspace. Writes the text to a JSON report after `secs`.
//!   edit_input_check <report.json> <secs>
use fltk::{app, prelude::*};
use fltk_omarchy::edit::TextArea;
use fltk_omarchy::layout::FlexExt;
use fltk_omarchy::{App, AppWindow};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let report = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "build/edit-input.json".into());
    let secs: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(6.0);
    let app = App::new("dev.omarchy.fltk.editinput");
    let mut win = AppWindow::new("edit-input-check", 420, 240);
    win.set_pos(100, 100);
    let mut body = win.body();
    // `--fltk`: fltk's own MultilineInput, to compare the platform path.
    let plain = args.iter().any(|a| a == "--fltk");
    let area = TextArea::new().lines(3, 6);
    let mut input = fltk::input::MultilineInput::default().with_size(380, 80);
    if plain {
        area.clone().hide();
        body.fixed(&input, 80);
    } else {
        input.hide();
        body.fit(&*area);
    }
    win.end();
    win.show();
    let _ = if plain {
        input.take_focus()
    } else {
        area.clone().take_focus()
    };
    let a2 = area.clone();
    app::add_timeout3(secs, move |_| {
        let v = if plain { input.value() } else { a2.value() };
        let esc: String = v
            .chars()
            .flat_map(|c| c.escape_unicode().collect::<Vec<_>>())
            .collect();
        let json = format!(
            "{{\"text\":{:?},\"escaped\":{:?},\"graphemes\":{}}}\n",
            v,
            esc,
            a2.with_buffer(|b| b.graphemes())
        );
        std::fs::write(&report, &json).expect("report");
        app::quit();
    });
    app.run();
}
