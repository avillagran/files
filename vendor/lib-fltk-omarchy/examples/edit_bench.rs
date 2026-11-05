//! TextArea / Composer costs in a shown window: typing latency (key
//! handling + redraw), a 100 KB paste, typing inside one 100 KB paragraph,
//! idle CPU and memory. Keys go through `focus::press` (no OS input), so
//! it runs on a hidden workspace (`script/edit-bench`) or in Xvfb.
//!   edit_bench <report.json>
use fltk::{app, prelude::*};
use fltk_omarchy::edit::{CompletionItem, Composer, TextArea, Trigger};
use fltk_omarchy::focus;
use fltk_omarchy::layout::FlexExt;
use fltk_omarchy::{App, AppWindow, State};
use std::time::{Duration, Instant};

fn pss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/smaps_rollup")
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with("Pss:"))
        .filter_map(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
        .sum()
}

fn cpu_s() -> f64 {
    fltk_omarchy::platform::process_cpu_time().map_or(0.0, |d| d.as_secs_f64())
}

/// Presses `key` n times, flushing (drawing) after each; (avg, p95, max) in µs.
fn type_keys(key: &str, n: usize) -> (u128, u128, u128) {
    let mut v: Vec<u128> = (0..n)
        .map(|_| {
            let t = Instant::now();
            focus::press(key);
            app::flush();
            t.elapsed().as_micros()
        })
        .collect();
    v.sort_unstable();
    (
        v.iter().sum::<u128>() / n as u128,
        v[n * 95 / 100],
        v[n - 1],
    )
}

fn main() {
    let report = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "build/edit-bench.json".into());
    let app = App::new("dev.omarchy.fltk.editbench");
    fltk_omarchy::anim::set_enabled(false);
    let mut win = AppWindow::new("omarchy-fltk-edit-bench", 720, 640);
    let mut body = win.body();
    let notes = TextArea::new().lines(4, 16);
    body.fit(&*notes);
    let draft = State::new(String::new());
    let composer = Composer::new()
        .bind(&draft)
        .trigger('@', Trigger::WordStart, |q, r| {
            r.set(
                (0..40)
                    .map(|i| CompletionItem::new(&format!("src/file{i}.rs")).chip(&q.text))
                    .collect(),
            );
        });
    body.fit(&*composer);
    win.end();
    win.show();
    let pss0 = pss_kb();
    let (n2, c2, w2) = (notes.clone(), composer.clone(), win.clone());
    app::add_timeout3(0.8, move |_| {
        let mut out = Vec::new();
        let _ = c2.area().take_focus();
        let (a, p, m) = type_keys("a", 300);
        out.push(format!(
            "\"composer_key_us\":{{\"avg\":{a},\"p95\":{p},\"max\":{m}}}"
        ));
        // Completion list open while typing.
        focus::press("Space");
        focus::type_text("@");
        let (a, p, m) = type_keys("s", 50);
        out.push(format!(
            "\"composer_key_with_popup_us\":{{\"avg\":{a},\"p95\":{p},\"max\":{m}}}"
        ));
        focus::press("Escape");
        // 100 KB paste into the notes area, then typing in it.
        let _ = n2.clone().take_focus();
        let para =
            "The quick brown fox jumps over the lazy dog, 日本語のテキストも折り返す。 ".repeat(12);
        let text = format!("{para}\n").repeat(100_000 / para.len() + 1);
        let t = Instant::now();
        n2.insert(&text);
        app::flush();
        out.push(format!(
            "\"paste_100kb_ms\":{:.2},\"paste_bytes\":{}",
            t.elapsed().as_secs_f64() * 1e3,
            text.len()
        ));
        let (a, p, m) = type_keys("x", 200);
        out.push(format!(
            "\"key_in_100kb_us\":{{\"avg\":{a},\"p95\":{p},\"max\":{m}}}"
        ));
        focus::press("Ctrl+Home");
        let (a, p, m) = type_keys("x", 200);
        out.push(format!(
            "\"key_at_start_of_100kb_us\":{{\"avg\":{a},\"p95\":{p},\"max\":{m}}}"
        ));
        // One 100 KB paragraph (no newlines): the worst case for wrapping.
        let one = para.repeat(100_000 / para.len() + 1);
        let t = Instant::now();
        n2.reset(&one);
        app::flush();
        out.push(format!(
            "\"set_100kb_one_paragraph_ms\":{:.2}",
            t.elapsed().as_secs_f64() * 1e3
        ));
        let (a, p, m) = type_keys("y", 200);
        out.push(format!(
            "\"key_end_of_one_paragraph_us\":{{\"avg\":{a},\"p95\":{p},\"max\":{m}}}"
        ));
        focus::press("Ctrl+Home");
        let (a, p, m) = type_keys("y", 200);
        out.push(format!(
            "\"key_start_of_one_paragraph_us\":{{\"avg\":{a},\"p95\":{p},\"max\":{m}}}"
        ));
        let t = Instant::now();
        for _ in 0..50 {
            focus::press("PageDown");
            app::flush();
        }
        out.push(format!(
            "\"pagedown_redraw_us\":{}",
            t.elapsed().as_micros() / 50
        ));
        let report = report.clone();
        let w3 = w2.clone();
        // Idle: nothing scheduled, no caret blink.
        app::add_timeout3(1.0, move |_| {
            let (c0, t0) = (cpu_s(), Instant::now());
            let (report, out, w4) = (report.clone(), out.clone(), w3.clone());
            app::add_timeout3(3.0, move |_| {
                let idle = 100.0 * (cpu_s() - c0) / t0.elapsed().as_secs_f64();
                let mut out = out.clone();
                out.push(format!(
                    "\"idle_cpu_pct\":{idle:.2},\"pss_kb_start\":{pss0},\"pss_kb_end\":{}",
                    pss_kb()
                ));
                let backend = if std::env::var_os("WAYLAND_DISPLAY").is_some()
                    && std::env::var("FLTK_BACKEND").as_deref() != Ok("x11")
                {
                    "wayland"
                } else {
                    "x11"
                };
                out.push(format!(
                    "\"backend\":\"{backend}\",\"build\":\"{}\"",
                    if cfg!(debug_assertions) {
                        "debug"
                    } else {
                        "release"
                    }
                ));
                let json = format!("{{{}}}\n", out.join(","));
                std::fs::write(&report, &json).expect("report");
                print!("{json}");
                let mut w = w4.clone();
                w.hide();
                app::quit();
            });
        });
    });
    let _ = Duration::ZERO;
    app.run();
}
