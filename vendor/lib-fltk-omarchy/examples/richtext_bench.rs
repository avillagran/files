//! Chat-stream benchmark for `RichText`: a transcript of N messages, then
//! Markdown streamed into the last message, then idle again. The process
//! measures itself (CPU ticks and PSS from /proc on Linux; zeros elsewhere)
//! and writes one JSON line to `--out`.
//!
//!   cargo run --release --example richtext_bench -- --messages 5000 --rate 1000000 --hz 60 --secs 10 --out build/rt.json
//!   script/richtext-bench        # same, as a real window on a hidden Hyprland workspace
//!
//! Phases: build (parse the transcript), settle 1.5 s, idle 3 s, stream
//! `secs` s (`--via timer`: `push_markdown` from a UI timer at `hz`;
//! `--via sender`: a worker thread pushes ~8-byte tokens through
//! `TextSender`, which applies them at most 60 times a second), idle 3 s.
//! `--lang ar`: the same transcript and stream in Arabic with Latin words
//! and numbers (right-to-left blocks through the bidi layout).
use fltk::app;
use fltk_omarchy::text::{Block, ColorRole, RichText, Style};
use fltk_omarchy::{App, AppWindow};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

const AGENT: &str = r#"Sure — here is what changed in `src/main.rs` and **why**:

1. The parser now keeps an *open tail* and commits closed blocks.
2. Layout is cached per block; see [the docs](https://example.com/docs).
   - [x] tests pass
   - [ ] benchmark the stream

```rust
fn push(&mut self, more: &str) -> Update {
    self.tail.push_str(more); // O(tail)
    let parsed = convert(&self.tail);
    Update { keep: self.committed, blocks: parsed.blocks }
}
```

| File | Lines | Status |
|------|------:|--------|
| main.rs | 120 | modified |
| text.rs | 980 | new |

> Note: long lines wrap; selection works across blocks.
"#;

const STREAM: &[&str] = &[
    "Streaming a long answer with **bold**, *italic*, `inline code` and a link to https://example.com/page in the middle of a sentence that keeps going for a while. ",
    "More words arrive token by token, each one appended to the last paragraph of the message. ",
    "\n\n",
    "- a list item with `code`\n- another item\n- third item with [a link](https://x.dev)\n\n",
    "```python\nfor i in range(10):\n    print(i)  # comment\n```\n\n",
    "Another paragraph of plain text to fill the transcript with something realistic to lay out and draw. ",
    "\n\n",
];

const AGENT_AR: &str = r#"بالتأكيد — هذا ما تغيّر في `src/main.rs` و**السبب**:

1. المحلل يحتفظ الآن *بذيل مفتوح* ويثبّت الكتل المغلقة.
2. التخطيط مخزّن لكل كتلة؛ راجع [التوثيق](https://example.com/docs).
   - [x] الاختبارات تنجح
   - [ ] قياس الأداء للبث

```rust
fn push(&mut self, more: &str) -> Update {
    self.tail.push_str(more); // O(tail)
}
```

| الملف | الأسطر | الحالة |
|------|------:|--------|
| main.rs | 120 | معدّل |
| text.rs | 980 | جديد |

> ملاحظة: الأسطر الطويلة تلتف؛ التحديد يعمل عبر الكتل.
"#;

const STREAM_AR: &[&str] = &[
    "بث إجابة طويلة مع **غامق** و*مائل* و`code` ورابط إلى https://example.com/page في منتصف جملة تستمر لبعض الوقت مع 42 رقمًا. ",
    "تصل كلمات أخرى رمزًا برمز، وكل واحدة تُلحق بآخر فقرة من الرسالة مثل RichText و Markdown. ",
    "\n\n",
    "- عنصر في قائمة مع `code`\n- عنصر آخر\n- عنصر ثالث مع [رابط](https://x.dev)\n\n",
    "```python\nfor i in range(10):\n    print(i)  # comment\n```\n\n",
    "فقرة أخرى من نص عادي لملء المحادثة بشيء واقعي للتخطيط والرسم في عام 2026. ",
    "\n\n",
];

fn arg(args: &[String], name: &str, default: &str) -> String {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| default.into())
}

fn proc_field(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn cpu_ticks() -> u64 {
    let s = proc_field("/proc/self/stat");
    let f: Vec<&str> = s
        .rsplit(") ")
        .next()
        .unwrap_or("")
        .split_whitespace()
        .collect();
    f.get(11).and_then(|x| x.parse::<u64>().ok()).unwrap_or(0)
        + f.get(12).and_then(|x| x.parse::<u64>().ok()).unwrap_or(0)
}

fn pss_kb() -> u64 {
    proc_field("/proc/self/smaps_rollup")
        .lines()
        .filter(|l| l.starts_with("Pss:") || l.starts_with("SwapPss:"))
        .filter_map(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
        .sum()
}

#[derive(Default)]
struct Run {
    idle1_cpu: f64,
    idle2_cpu: f64,
    stream_cpu: f64,
    pss_idle_kb: u64,
    pss_stream_kb: u64,
    pushes: u64,
    push_total: Duration,
    push_max: Duration,
    bytes: usize,
    phase_start: Option<(Instant, u64)>,
    paint0: (u64, Duration),
    frames: u64,
    paint: Duration,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let messages: usize = arg(&args, "--messages", "5000").parse().unwrap();
    let rate: usize = arg(&args, "--rate", "1000000").parse().unwrap();
    let hz: f64 = arg(&args, "--hz", "60").parse().unwrap();
    let secs: f64 = arg(&args, "--secs", "10").parse().unwrap();
    let via = arg(&args, "--via", "timer");
    let arabic = arg(&args, "--lang", "en") == "ar";
    let out = arg(&args, "--out", "build/richtext-bench.json");
    let title = args
        .last()
        .cloned()
        .filter(|a| !a.starts_with("--") && args.len() > 1)
        .unwrap_or_else(|| "richtext-bench".into());
    let app = App::without_theme_watch("dev.omarchy.fltk.richtext-bench");
    let mut win = AppWindow::new(&title, 800, 900);
    let view = RichText::new().stick_to_bottom(true);
    win.end();

    let t0 = Instant::now();
    let header = |name: &str, role: ColorRole| {
        Block::paragraph("").push(name, &Style::plain().bold().color(role))
    };
    let mut transcript_bytes = 0;
    for k in 0..messages {
        if k % 2 == 0 {
            view.append_block(header("You", ColorRole::Accent));
            let q = if arabic {
                format!("السؤال {k}: كيف أبث **Markdown** إلى عرض `RichText`؟")
            } else {
                format!("Question {k}: how do I stream **Markdown** into a `RichText` view?")
            };
            transcript_bytes += q.len();
            view.append_markdown(&q);
        } else {
            view.append_block(header("Agent", ColorRole::Green));
            let a = if arabic { AGENT_AR } else { AGENT };
            transcript_bytes += a.len();
            view.append_markdown(a);
        }
    }
    view.finish();
    let build_ms = t0.elapsed().as_secs_f64() * 1000.0;
    view.append_block(header("Agent", ColorRole::Green));
    view.append_markdown("");
    win.show();

    let run = Rc::new(RefCell::new(Run::default()));
    let tps = 100.0; // USER_HZ
                     // Phase 1: idle.
    let (r, v, via2) = (run.clone(), view.clone(), via.clone());
    app::add_timeout3(1.5, move |_| {
        r.borrow_mut().phase_start = Some((Instant::now(), cpu_ticks()));
        let (r, v, via) = (r.clone(), v.clone(), via2.clone());
        app::add_timeout3(3.0, move |_| {
            {
                let mut s = r.borrow_mut();
                let (_, c0) = s.phase_start.take().unwrap();
                s.idle1_cpu = (cpu_ticks() - c0) as f64 / tps;
                s.pss_idle_kb = pss_kb();
                s.phase_start = Some((Instant::now(), cpu_ticks()));
                s.paint0 = v.paint_stats();
            }
            start_stream(r.clone(), v.clone(), rate, hz, secs, &via, arabic);
        });
    });
    // Finish: after the stream and 3 s of idle.
    let (r, v, out2, via) = (run.clone(), view.clone(), out.clone(), via.clone());
    app::add_timeout3(1.5 + 3.0 + secs + 0.2, move |_| {
        {
            let mut s = r.borrow_mut();
            let (t, c0) = s.phase_start.take().unwrap();
            let el = t.elapsed().as_secs_f64();
            s.stream_cpu = (cpu_ticks() - c0) as f64 / tps / el;
            let (f1, p1) = v.paint_stats();
            s.frames = f1 - s.paint0.0;
            s.paint = p1 - s.paint0.1;
            s.pss_stream_kb = pss_kb();
            s.phase_start = Some((Instant::now(), cpu_ticks()));
        }
        let (r, v, out2, via) = (r.clone(), v.clone(), out2.clone(), via.clone());
        app::add_timeout3(3.0, move |_| {
            let mut s = r.borrow_mut();
            let (_, c0) = s.phase_start.take().unwrap();
            s.idle2_cpu = (cpu_ticks() - c0) as f64 / tps;
            let json = format!(
                "{{\"lang\":\"{}\",\"messages\":{messages},\"blocks\":{},\"transcript_kb\":{},\"build_ms\":{build_ms:.0},\"via\":\"{}\",\"rate_bps\":{rate},\"hz\":{hz},\"secs\":{secs},\"streamed_kb\":{},\"pushes\":{},\"push_avg_us\":{:.0},\"push_max_us\":{},\"laid_out\":{},\"idle_cpu_pct_before\":{:.1},\"stream_cpu_pct\":{:.1},\"idle_cpu_pct_after\":{:.1},\"pss_idle_kb\":{},\"pss_after_stream_kb\":{},\"at_bottom\":{},\"frames\":{},\"paint_avg_us\":{:.0},\"paint_cpu_pct\":{:.1},\"push_cpu_pct\":{:.1}}}",
                if arabic { "ar" } else { "en" },
                v.block_count(),
                transcript_bytes / 1024,
                via,
                s.bytes / 1024,
                s.pushes,
                if s.pushes > 0 { s.push_total.as_secs_f64() * 1e6 / s.pushes as f64 } else { 0.0 },
                s.push_max.as_micros(),
                v.blocks_laid_out(),
                s.idle1_cpu / 3.0 * 100.0,
                s.stream_cpu * 100.0,
                s.idle2_cpu / 3.0 * 100.0,
                s.pss_idle_kb,
                s.pss_stream_kb,
                v.is_at_bottom(),
                s.frames,
                if s.frames > 0 { s.paint.as_secs_f64() * 1e6 / s.frames as f64 } else { 0.0 },
                s.paint.as_secs_f64() / secs * 100.0,
                s.push_total.as_secs_f64() / secs * 100.0,
            );
            println!("{json}");
            if let Some(dir) = std::path::Path::new(&out2).parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            std::fs::write(&out2, json + "\n").expect("write report");
            app::quit();
        });
    });
    app.run();
}

fn start_stream(run: Rc<RefCell<Run>>, view: RichText, rate: usize, hz: f64, secs: f64, via: &str, arabic: bool) {
    let source: String = (if arabic { STREAM_AR } else { STREAM }).iter().cycle().take(400).copied().collect();
    let end = Instant::now() + Duration::from_secs_f64(secs);
    if via == "sender" {
        let tx = view.sender();
        let per_ms = (rate / 1000).max(1);
        std::thread::spawn(move || {
            let mut at = 0;
            let mut sent = 0usize;
            let t0 = Instant::now();
            while Instant::now() < end {
                // ~8-byte tokens, paced to the byte rate.
                let due = (t0.elapsed().as_secs_f64() * rate as f64) as usize;
                while sent < due {
                    let mut e = (at + 8).min(source.len());
                    while !source.is_char_boundary(e) {
                        e += 1;
                    }
                    tx.push(&source[at..e]);
                    sent += e - at;
                    at = if e >= source.len() { 0 } else { e };
                }
                let _ = per_ms;
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        run.borrow_mut().bytes = (rate as f64 * secs) as usize;
        return;
    }
    let chunk = (rate as f64 / hz) as usize;
    let at = Rc::new(RefCell::new(0usize));
    app::add_timeout3(1.0 / hz, move |h| {
        if Instant::now() >= end {
            return;
        }
        let mut a = at.borrow_mut();
        let mut piece = String::with_capacity(chunk);
        while piece.len() < chunk {
            let mut e = (*a + chunk - piece.len()).min(source.len());
            while !source.is_char_boundary(e) {
                e += 1;
            }
            piece.push_str(&source[*a..e]);
            *a = if e >= source.len() { 0 } else { e };
        }
        let t = Instant::now();
        view.push_markdown(&piece);
        let d = t.elapsed();
        {
            let mut s = run.borrow_mut();
            s.pushes += 1;
            s.push_total += d;
            s.push_max = s.push_max.max(d);
            s.bytes += piece.len();
        }
        app::repeat_timeout3(1.0 / hz, h);
    });
}
