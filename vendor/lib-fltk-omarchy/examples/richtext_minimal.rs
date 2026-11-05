//! Size reference: the smallest app with a RichText (compare with
//! bench_minimal for the cost of the `text` module in a release binary).
use fltk_omarchy::text::RichText;
use fltk_omarchy::{App, AppWindow};

fn main() {
    let title = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "richtext-minimal".into());
    let app = App::without_theme_watch("dev.omarchy.fltk.richtext-minimal");
    let mut win = AppWindow::new(&title, 520, 400);
    let view = RichText::new();
    win.end();
    view.set_markdown(
        "Hello from **lib-fltk-omarchy**: `RichText` with [a link](https://example.com).",
    );
    win.show();
    app.run();
}
