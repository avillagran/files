//! o3-style streaming load: a 2 000-thread list and an open conversation
//! whose text grows by ~1 MB/s, fed by a background thread at `rate`
//! events/s (60 % status patches, 25 % new messages that bump a thread to
//! the top, 15 % reorders; 40 % of them hit the visible rows). Three ways
//! to get the events to the UI:
//!
//! - `naive`: every event re-sends the whole state (thread list + message
//!   text) as snapshots; the UI sets a `State<Vec<Thread>>` and a
//!   `State<String>` and redraws a count-based `List`.
//! - `diff`: the same snapshots, reconciled with `ListModel::sync_from` and
//!   `State::set_text` (the protocol is unchanged, the UI stops rebuilding).
//! - `feed`: the producer sends diffs through `model::Feed` and the text
//!   through `TextFeed` (the protocol sends changes).
//! - `feed60`: `feed` with `Feed::max_rate(60.0)`.
//!
//! Run by `script/stream-bench` on a hidden Hyprland workspace:
//!   stream_bench <mode> <events/s> <seconds> <report.json> [title] [width height]
//! Writes one JSON line: CPU (whole process, and the UI thread alone), PSS,
//! UI-thread CPU per painted frame (apply + paint), rows built, bytes handed to the UI
//! thread, notifications, and the idle CPU after the stream stops.
use fltk::{app, draw, enums::*, prelude::*, widget::Widget};
use fltk_omarchy::list::{List, ListRow};
use fltk_omarchy::model::{Diff, ListModel};
use fltk_omarchy::state::Edit;
use fltk_omarchy::{task, theme, App, AppWindow, Mode, State};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const THREADS: usize = 2000;
const BYTES_PER_S: usize = 1_000_000;
/// A message is reset (a new one starts) past this size.
const MESSAGE_MAX: usize = 256 * 1024;
const STATUSES: [&str; 4] = ["idle", "running", "waiting", "done"];

#[derive(Debug, Clone, PartialEq)]
struct Thread {
    id: u64,
    title: String,
    status: &'static str,
    unread: u32,
    preview: String,
    updated: u64,
}

impl Thread {
    fn row(&self) -> ListRow {
        ListRow::new(&self.title).detail(&format!("{} · {}", self.status, self.unread))
    }
    fn bytes(&self) -> usize {
        std::mem::size_of::<Thread>() + self.title.len() + self.preview.len()
    }
}

fn pss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/smaps_rollup")
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with("Pss:") || l.starts_with("SwapPss:"))
        .filter_map(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
        .sum()
}

/// CPU time of the calling (UI) thread: apply + paint, wherever fltk runs them.
#[cfg(unix)]
fn thread_cpu_ms() -> f64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) };
    ts.tv_sec as f64 * 1000.0 + ts.tv_nsec as f64 / 1e6
}

thread_local! {
    static PAINTS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Without a per-thread clock (Windows): the whole process.
#[cfg(not(unix))]
fn thread_cpu_ms() -> f64 {
    cpu_s() * 1000.0
}

#[cfg(not(unix))]
fn cpu_s() -> f64 {
    fltk_omarchy::platform::process_cpu_time().map_or(0.0, |d| d.as_secs_f64())
}

#[cfg(unix)]
fn cpu_s() -> f64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut ts) };
    ts.tv_sec as f64 + ts.tv_nsec as f64 / 1e9
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// What a consumer of streamed text does: keeps line starts, and on an
/// append only scans the new tail. Draws the last lines.
#[derive(Default)]
struct Tail {
    lines: Vec<usize>,
    scanned: usize,
}

impl Tail {
    fn apply(&mut self, text: &str, edit: Edit) {
        let from = match edit {
            Edit::Appended { from } if from == self.scanned => from,
            _ => {
                self.lines.clear();
                self.lines.push(0);
                0
            }
        };
        for (i, b) in text.as_bytes()[from..].iter().enumerate() {
            if *b == b'\n' {
                self.lines.push(from + i + 1);
            }
        }
        self.scanned = text.len();
    }
}

/// UI-thread objects reached by closures posted from the producer.
enum Ui {
    Naive {
        threads: State<Vec<Thread>>,
        text: State<String>,
    },
    Diff {
        model: ListModel<u64, Thread>,
        text: State<String>,
    },
}

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mode = a.get(1).cloned().unwrap_or_else(|| "feed".into());
    let rate: f64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(60.0);
    let secs: f64 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(10.0);
    let out = a
        .get(4)
        .cloned()
        .unwrap_or_else(|| "build/stream-bench.json".into());
    let title = a
        .get(5)
        .cloned()
        .unwrap_or_else(|| format!("stream-bench-{mode}"));

    let size: Vec<i32> = a[6.min(a.len())..]
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect();
    let (width, height) = (*size.first().unwrap_or(&520), *size.get(1).unwrap_or(&400));

    let app = App::without_theme_watch("dev.omarchy.fltk.streambench");
    let mut win = AppWindow::new(&title, width, height);
    let body = win.body();

    let mut rng = Rng(0x2545F4914F6CDD1D);
    let initial: Vec<Thread> = (0..THREADS as u64)
        .map(|i| Thread {
            id: i,
            title: format!("Thread {i:04} · project-{}", i % 17),
            status: STATUSES[i as usize % 4],
            unread: 0,
            preview: "x".repeat(100),
            updated: 0,
        })
        .collect();

    // The list (stretches in the body) and the message tail (fixed height).
    let text = State::new(String::new());
    let model = ListModel::with_items(|t: &Thread| t.id, initial.clone());
    let naive_threads = State::new(initial.clone());
    let list = match mode.as_str() {
        "naive" => {
            let count = naive_threads.map(|v| v.len());
            let th = naive_threads.clone();
            List::new(&count, move |i| {
                th.with(|v| v.get(i).map(Thread::row).unwrap_or_default())
            })
        }
        _ => List::from_model(&model, Thread::row),
    };
    let naive_notes = Rc::new(std::cell::Cell::new(0u64));
    if mode == "naive" {
        // The count does not change: a naive view redraws on every snapshot.
        let mut lw = list.clone();
        let nn = naive_notes.clone();
        naive_threads
            .subscribe(move |_| {
                nn.set(nn.get() + 1);
                lw.redraw()
            })
            .forever();
    }
    let tail = Rc::new(RefCell::new(Tail::default()));
    let mut tail_w = Widget::default();
    tail_w.set_frame(FrameType::NoBox);
    {
        let (t2, tx) = (tail.clone(), text.clone());
        tail_w.draw(move |w| {
            PAINTS.with(|p| p.set(p.get() + 1));
            theme::with(|t| {
                draw::set_font(Font::Courier, t.font.body_small);
                draw::set_draw_color(t.palette.foreground.fl());
                let tail = t2.borrow();
                tx.with(|s| {
                    let n = tail.lines.len();
                    for (k, &start) in tail.lines[n.saturating_sub(5)..].iter().enumerate() {
                        let end = s[start..]
                            .find('\n')
                            .map_or(s.len(), |e| start + e)
                            .min(start + 90);
                        let end = (start..=end)
                            .rev()
                            .find(|e| s.is_char_boundary(*e))
                            .unwrap_or(start);
                        draw::draw_text2(
                            &s[start..end],
                            w.x(),
                            w.y() + k as i32 * (t.font.body_small + 6),
                            w.w(),
                            t.font.body_small + 6,
                            Align::Left,
                        );
                    }
                });
            });
        });
    }
    {
        let (t2, mut w2) = (tail.clone(), tail_w.clone());
        text.subscribe_edit(move |s, e| {
            t2.borrow_mut().apply(s, e);
            w2.redraw();
        })
        .forever();
    }
    let mut body = body;
    body.fixed(&tail_w, 5 * 18);
    win.end();
    win.set_mode(Mode::Floating);
    win.show();

    UI.with(|u| {
        *u.borrow_mut() = Some(match mode.as_str() {
            "naive" => Ui::Naive {
                threads: naive_threads.clone(),
                text: text.clone(),
            },
            _ => Ui::Diff {
                model: model.clone(),
                text: text.clone(),
            },
        })
    });

    // ------------------------------------------------------------ producer
    let feed = model.feed();
    if mode == "feed60" {
        feed.set_max_rate(60.0);
    }
    let text_feed = text.text_feed();
    if mode == "feed60" {
        text_feed.set_max_rate(60.0);
    }
    let ui_bytes = Arc::new(AtomicU64::new(0));
    let events = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let corpus: String = {
        let words = [
            "agent", "tool", "call", "diff", "file", "patch", "test", "build", "ok", "error",
            "token", "stream", "cargo", "fltk", "model",
        ];
        let mut s = String::new();
        let mut col = 0;
        while s.len() < 64 * 1024 {
            let w = words[rng.below(words.len())];
            s.push_str(w);
            col += w.len() + 1;
            if col > 72 {
                s.push('\n');
                col = 0
            } else {
                s.push(' ')
            }
        }
        s
    };
    let producer = {
        let (mode, ui_bytes, events, stop) =
            (mode.clone(), ui_bytes.clone(), events.clone(), stop.clone());
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            let mut rng = Rng(0x9E3779B97F4A7C15);
            let mut threads = initial;
            let mut message = String::new();
            let chunk = (BYTES_PER_S as f64 / rate) as usize;
            let mut cpos = 0usize;
            let period = Duration::from_secs_f64(1.0 / rate);
            let mut next = Instant::now();
            let mut n = 0u64;
            while !stop.load(Ordering::Relaxed) {
                n += 1;
                // The event.
                let idx = if rng.below(10) < 4 {
                    rng.below(15)
                } else {
                    rng.below(threads.len())
                };
                let id = threads[idx].id;
                let kind = rng.below(100);
                let mut diffs: Vec<Diff<u64, Thread>> = Vec::new();
                if kind < 60 {
                    let st = STATUSES[rng.below(4)];
                    threads[idx].status = st;
                    diffs.push(Diff::patch(id, move |t: &mut Thread| t.status = st));
                } else if kind < 85 {
                    let start = cpos % (corpus.len() - 200);
                    let start = (start..).find(|i| corpus.is_char_boundary(*i)).unwrap();
                    let preview: String = corpus[start..].chars().take(100).collect();
                    let t = &mut threads[idx];
                    t.unread += 1;
                    t.updated = n;
                    t.preview = preview;
                    let item = t.clone();
                    let moved = threads.remove(idx);
                    threads.insert(0, moved);
                    diffs.push(Diff::Upsert(item));
                    diffs.push(Diff::Move(id, 0));
                } else {
                    let moved = threads.remove(idx);
                    threads.insert(0, moved);
                    diffs.push(Diff::Move(id, 0));
                }
                // ~1 MB/s of message text.
                let mut piece = String::with_capacity(chunk);
                while piece.len() < chunk {
                    let start = cpos % corpus.len();
                    let take = (chunk - piece.len()).min(corpus.len() - start);
                    let end = (start + take..=corpus.len())
                        .find(|e| corpus.is_char_boundary(*e))
                        .unwrap();
                    piece.push_str(&corpus[start..end]);
                    cpos = end;
                }
                if message.len() + piece.len() > MESSAGE_MAX {
                    message.clear();
                    if mode.starts_with("feed") {
                        text_feed.set(String::new());
                    }
                }
                message.push_str(&piece);
                match mode.as_str() {
                    "naive" | "diff" => {
                        let snap = threads.clone();
                        let msg = message.clone();
                        let bytes = snap.iter().map(Thread::bytes).sum::<usize>() + msg.len();
                        ui_bytes.fetch_add(bytes as u64, Ordering::Relaxed);
                        task::post(move || {
                            UI.with(|u| match u.borrow().as_ref() {
                                Some(Ui::Naive { threads, text }) => {
                                    threads.set(snap);
                                    text.set(msg);
                                }
                                Some(Ui::Diff { model, text }) => {
                                    model.sync_from(snap);
                                    text.set_text(&msg);
                                }
                                None => {}
                            })
                        });
                    }
                    _ => {
                        let bytes = piece.len()
                            + diffs.len() * std::mem::size_of::<Diff<u64, Thread>>()
                            + if (60..85).contains(&kind) {
                                threads[0].bytes()
                            } else {
                                0
                            };
                        ui_bytes.fetch_add(bytes as u64, Ordering::Relaxed);
                        feed.push_all(diffs);
                        text_feed.push_str(&piece);
                    }
                }
                events.store(n, Ordering::Relaxed);
                next += period;
                let now = Instant::now();
                if next > now {
                    std::thread::sleep(next - now);
                } else {
                    next = now;
                }
            }
        })
    };

    // -------------------------------------------------------- measurement
    let notifications = || match mode.as_str() {
        "naive" => naive_notes.get(),
        _ => model.notifications(),
    };
    let t_show = Instant::now();
    let warm = Duration::from_millis(1500);
    let run_end = warm + Duration::from_secs_f64(secs);
    let idle_from = run_end + Duration::from_millis(500);
    let idle_end = idle_from + Duration::from_secs(3);
    let mut frames: Vec<f64> = Vec::new();
    let mut paints = 0u64;
    let mut busy = 0.0f64;
    let mut start: Option<(f64, u64, usize, u64, u64)> = None;
    let mut measured: Option<(f64, f64, u64, usize, u64, u64, f64)> = None;
    let mut idle_c0: Option<f64> = None;
    let mut pss_max = 0u64;
    let mut last_pss = Instant::now();
    // One-shot wakes at the phase boundaries and once a second during the
    // run (PSS samples): no periodic timer in the idle window.
    let mut wake = vec![warm, run_end, idle_from, idle_end];
    let mut s = warm;
    while s < run_end {
        s += Duration::from_secs(1);
        wake.push(s);
    }
    for t in wake {
        app::add_timeout3(t.as_secs_f64() + 0.001, |_| {});
    }
    // Per loop iteration: UI-thread CPU (not wall time: sleeping in wait
    // costs nothing) and whether the message view was painted. With a rate
    // cap the apply runs in an fltk timeout and the paint inside wait().
    loop {
        let c0 = thread_cpu_ms();
        let p0 = PAINTS.with(|p| p.get());
        if !app::wait() {
            break;
        }
        task::drain();
        fltk_omarchy::app::flush();
        let dt = thread_cpu_ms() - c0;
        let painted = PAINTS.with(|p| p.get()) > p0;
        let el = t_show.elapsed();
        if el >= warm && el < run_end {
            if start.is_none() {
                start = Some((
                    cpu_s(),
                    events.load(Ordering::Relaxed),
                    list.rows_built(),
                    ui_bytes.load(Ordering::Relaxed),
                    notifications(),
                ));
            }
            if painted {
                paints += 1;
                frames.push(dt);
            }
            busy += dt;
            if last_pss.elapsed() > Duration::from_secs(1) {
                pss_max = pss_max.max(pss_kb());
                last_pss = Instant::now();
            }
        } else if el >= run_end && measured.is_none() {
            let (c0, e0, r0, b0, n0) = start.expect("measured window");
            let wall = (el - warm).as_secs_f64();
            measured = Some((
                cpu_s() - c0,
                wall,
                events.load(Ordering::Relaxed) - e0,
                list.rows_built() - r0,
                ui_bytes.load(Ordering::Relaxed) - b0,
                notifications() - n0,
                busy,
            ));
            pss_max = pss_max.max(pss_kb());
            stop.store(true, Ordering::Relaxed);
        } else if el >= idle_from && idle_c0.is_none() {
            idle_c0 = Some(cpu_s());
        } else if el >= idle_end {
            break;
        }
    }
    let idle_cpu = cpu_s() - idle_c0.unwrap_or_else(cpu_s);
    producer.join().ok();
    let (cpu, wall, evs, rows, bytes, notes, busy) = measured.expect("run finished");
    frames.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |p: f64| {
        if frames.is_empty() {
            0.0
        } else {
            frames[((frames.len() - 1) as f64 * p).round() as usize]
        }
    };
    let line = format!(
        "{{\"mode\":\"{mode}\",\"size\":\"{width}x{height}\",\"rate\":{rate},\"seconds\":{wall:.2},\"events\":{evs},\"events_per_s\":{:.1},\"cpu_pct\":{:.1},\"ui_thread_cpu_pct\":{:.1},\"paints_per_s\":{:.1},\"frame_cpu_ms_median\":{:.3},\"frame_cpu_ms_p95\":{:.3},\"frame_cpu_ms_max\":{:.3},\"rows_built_per_s\":{:.0},\"ui_mb_per_s\":{:.2},\"notifications\":{notes},\"pss_kb\":{},\"pss_max_kb\":{pss_max},\"idle_cpu_pct\":{:.2}}}",
        evs as f64 / wall, 100.0 * cpu / wall, 100.0 * busy / 1000.0 / wall, paints as f64 / wall,
        q(0.5), q(0.95), q(1.0), rows as f64 / wall, bytes as f64 / wall / 1e6, pss_kb(), 100.0 * idle_cpu / 3.0
    );
    println!("{line}");
    std::fs::write(&out, format!("{line}\n")).unwrap();
    win.hide();
    drop(app);
}
