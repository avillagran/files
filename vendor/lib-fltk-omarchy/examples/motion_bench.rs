//! Motion benchmark on a real window (Hyprland, hidden special workspace,
//! launched by `script/motion-bench`; never takes focus, no synthetic OS
//! input). A sidebar beside a 100 000-row list: the pointer resting on the
//! sidebar is simulated through the toolkit's own hover path
//! (`Sidebar::hover`, the code Enter/Leave run), so the hover intent, the
//! layout motion and the redraws are the real ones.
//!
//! Reports idle CPU before and after, and for each expand / collapse the
//! frames drawn, their intervals and the CPU used while moving.
//!   motion_bench <report.json> [cycles]
use fltk::{app, enums::*, prelude::*, widget::Widget};
use fltk_omarchy::layout::{self, FlexExt};
use fltk_omarchy::list::{List, ListRow};
use fltk_omarchy::widgets::*;
use fltk_omarchy::{anim, App, AppWindow, State};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

fn cpu() -> Duration {
    fltk_omarchy::platform::process_cpu_time().unwrap_or_default()
}

fn pump(d: Duration) {
    let end = Instant::now() + d;
    while Instant::now() < end {
        app::wait_for(0.005).ok();
    }
}

/// Idle: blocks in the event loop (no polling of our own).
fn idle(d: Duration) {
    let end = Instant::now() + d;
    while Instant::now() < end {
        app::wait_for((end - Instant::now()).as_secs_f64().min(1.0)).ok();
    }
}

fn pump_until(mut f: impl FnMut() -> bool, max: Duration) {
    let end = Instant::now() + max;
    while !f() && Instant::now() < end {
        app::wait_for(0.002).ok();
    }
}

#[derive(Default)]
struct Frames {
    at: Vec<Instant>,
    paint: Vec<Duration>,
}

fn stats(name: &str, f: &Frames, cpu: Duration, wall: Duration, width: (i32, i32)) -> String {
    let gaps: Vec<f64> =
        f.at.windows(2)
            .map(|w| (w[1] - w[0]).as_secs_f64() * 1000.0)
            .collect();
    let avg = if gaps.is_empty() {
        0.0
    } else {
        gaps.iter().sum::<f64>() / gaps.len() as f64
    };
    let max = gaps.iter().cloned().fold(0.0, f64::max);
    let paint: Vec<f64> = f.paint.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
    let pavg = if paint.is_empty() {
        0.0
    } else {
        paint.iter().sum::<f64>() / paint.len() as f64
    };
    let pmax = paint.iter().cloned().fold(0.0, f64::max);
    format!(
        "{{\"case\":\"{name}\",\"frames\":{},\"frame_interval_avg_ms\":{avg:.2},\"frame_interval_max_ms\":{max:.2},\"paint_avg_ms\":{pavg:.3},\"paint_max_ms\":{pmax:.3},\"cpu_ms\":{:.1},\"wall_ms\":{:.1},\"cpu_pct_of_core\":{:.1},\"width_from\":{},\"width_to\":{}}}",
        f.at.len(),
        cpu.as_secs_f64() * 1000.0,
        wall.as_secs_f64() * 1000.0,
        100.0 * cpu.as_secs_f64() / wall.as_secs_f64().max(1e-9),
        width.0,
        width.1
    )
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let report = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "build/motion-bench.json".into());
    let cycles: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10);
    let app = App::new("dev.omarchy.fltk.motionbench");
    // With OMARCHY_FLTK_ANIM set the system setting is never read (no watcher).
    let system = if std::env::var_os("OMARCHY_FLTK_ANIM").is_some() {
        None
    } else {
        anim::system()
    };
    let source = anim::source();
    let mut win = AppWindow::new("omarchy-fltk-motion-bench", 900, 560);
    let mut body = win.body();
    body.set_margin(0);
    let view = State::new(0usize);
    let mut row = layout::row();
    let side = Sidebar::new(
        vec![
            SidebarItem::new(glyph::FOLDER, "Files").badge("12"),
            SidebarItem::new(glyph::SEARCH, "Search"),
            SidebarItem::new(glyph::BELL, "Notifications").badge("3"),
            SidebarItem::new(glyph::MONITOR, "Displays"),
            SidebarItem::new(glyph::SETTINGS, "Settings").footer(),
        ],
        &view,
    );
    row.fit(&*side);
    let mut col = layout::column();
    // A probe drawn last in the content: one timestamp per frame.
    let frames: Rc<RefCell<Frames>> = Rc::default();
    let start: Rc<RefCell<Option<Instant>>> = Rc::default();
    let mut first = Widget::default().with_size(10, 1);
    first.set_frame(FrameType::NoBox);
    let s2 = start.clone();
    first.draw(move |_| *s2.borrow_mut() = Some(Instant::now()));
    col.fixed(&first, 1);
    let mut bar = layout::row();
    for l in ["New", "Open", "Share"] {
        bar.fit(&*Button::new(l).bordered());
    }
    bar.close_fitted();
    let list = List::new(&State::new(100_000usize), |i| {
        ListRow::new(&format!("Item {i}")).detail(&format!("{} KB", i * 3))
    });
    let _ = &list;
    let mut probe = Widget::default().with_size(10, 1);
    probe.set_frame(FrameType::NoBox);
    let (f2, s3) = (frames.clone(), start.clone());
    probe.draw(move |_| {
        let now = Instant::now();
        let mut f = f2.borrow_mut();
        f.at.push(now);
        if let Some(s) = s3.borrow_mut().take() {
            f.paint.push(now - s);
        }
    });
    col.fixed(&probe, 1);
    col.end();
    row.end();
    win.end();
    win.show();
    pump(Duration::from_millis(600));

    let mut out = vec![format!(
        "\"system_animations\":{},\"source\":\"{:?}\"",
        system.map_or("null".to_string(), |b| b.to_string()),
        source
    )];
    // Idle before.
    let c0 = cpu();
    idle(Duration::from_secs(5));
    out.push(format!(
        "\"idle_before_cpu_ms_per_5s\":{:.2}",
        (cpu() - c0).as_secs_f64() * 1000.0
    ));

    // The clock starts when the hover intent fires (the width target changes).
    let mark: Rc<RefCell<Option<(Duration, Instant)>>> = Rc::default();
    let (m2, f3) = (mark.clone(), frames.clone());
    let _sub = side.expanded().subscribe(move |_| {
        let mut f = f3.borrow_mut();
        f.at.clear();
        f.paint.clear();
        *m2.borrow_mut() = Some((cpu(), Instant::now()));
    });
    let mut runs = Vec::new();
    for mode in ["on", "off"] {
        anim::set_enabled(mode == "on");
        for _ in 0..cycles {
            for (dir, name) in [(true, "expand"), (false, "collapse")] {
                let from = side.w();
                *mark.borrow_mut() = None;
                // The hover path: intent delay, then the width target changes.
                side.hover(dir);
                pump_until(|| mark.borrow().is_some(), Duration::from_secs(2));
                pump_until(
                    || !frames.borrow().at.is_empty() && !layout::animating(),
                    Duration::from_secs(2),
                );
                // One more frame interval so the last frame is counted.
                pump(Duration::from_millis(20));
                let (c, t) = mark.borrow().unwrap_or((cpu(), Instant::now()));
                let (dc, dt) = (cpu() - c, t.elapsed());
                runs.push(stats(
                    &format!("{name}-{mode}"),
                    &frames.borrow(),
                    dc,
                    dt,
                    (from, side.w()),
                ));
                pump(Duration::from_millis(120));
            }
        }
    }
    anim::set_enabled(None);
    // Idle after: the shared timer must have stopped.
    pump(Duration::from_millis(300));
    let c1 = cpu();
    idle(Duration::from_secs(5));
    out.push(format!(
        "\"idle_after_cpu_ms_per_5s\":{:.2}",
        (cpu() - c1).as_secs_f64() * 1000.0
    ));
    out.push(format!("\"pending_after\":{}", anim::pending()));
    out.push(format!("\"runs\":[{}]", runs.join(",")));
    std::fs::write(&report, format!("{{{}}}\n", out.join(","))).unwrap();
    win.clone().hide();
    drop(app);
}
