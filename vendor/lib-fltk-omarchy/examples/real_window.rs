//! Real-window scenario for Hyprland, launched on a hidden special workspace
//! by `script/real-window` (never takes focus). Interactions are driven
//! through the public API (no synthetic OS input). Writes a JSON report and
//! screenshots of the shown window.
//!   real_window <report.json> <shots-dir>
use fltk::{app, prelude::*};
use fltk_omarchy::layout::{self, FlexExt};
use fltk_omarchy::list::{List, ListRow};
use fltk_omarchy::task::{Outcome, Task, TaskHandle};
use fltk_omarchy::widgets::*;
use fltk_omarchy::{combine, focus, hyprland, task, theme, App, AppWindow, Mode, State};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

fn pss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/smaps_rollup").unwrap_or_default().lines()
        .filter(|l| l.starts_with("Pss:") || l.starts_with("SwapPss:"))
        .filter_map(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok()).sum()
}

fn cpu_s() -> f64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut ts) };
    ts.tv_sec as f64 + ts.tv_nsec as f64 / 1e9
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (report_path, shots) = (args[1].clone(), args[2].clone());
    let t0 = Instant::now();
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let step = { let log = log.clone(); move |s: String| log.borrow_mut().push(format!("{{\"t\":{:.3},{s}}}", t0.elapsed().as_secs_f64())) };

    let app = App::new("dev.omarchy.fltk.itest");
    let mut win = AppWindow::new("omarchy-fltk-itest", 520, 420);
    let mut body = win.body();
    let name = State::new(String::new());
    let clicks = State::new(0u32);
    let progress = State::new(0.0);
    let status = State::new("Idle".to_string());
    let mut field = TextField::new().placeholder("Name").bind(&name);
    body.fit(&*field);
    let greet = combine(&name, &clicks, |n, c| format!("Hello {n} ({c})"));
    let greeting = Label::new("").bold().bind(&greet);
    body.fit(&*greeting);
    let mut r = layout::row();
    let c1 = clicks.clone();
    let add = Button::new("Add").primary().on_click(move || c1.update(|c| *c += 1));
    r.fit(&*add);
    r.fit(&*Label::new("").bind(&status));
    r.close_fitted();
    body.fit(&*Progress::new(&progress));
    let _list = List::new(&State::new(100_000usize), |i| ListRow::new(&format!("Item {i}")).detail(&format!("{} KB", i * 3)));
    win.end();
    win.show();

    let job: Rc<RefCell<Option<TaskHandle>>> = Rc::default();
    let report: Rc<RefCell<Vec<String>>> = Rc::default();
    let (st, rp) = (step.clone(), report.clone());
    app::add_timeout3(0.05, move |_| {
        rp.borrow_mut().push(format!("\"startup_to_loop_ms\":{}", t0.elapsed().as_millis()));
        st(format!("\"step\":\"shown\",\"backend\":\"{}\"", if std::env::var_os("WAYLAND_DISPLAY").is_some() && std::env::var("FLTK_BACKEND").as_deref() != Ok("x11") { "wayland" } else { "x11" }));
    });
    let (st, rp, mut w, sh, pr, stt, jb, cl, nm) = (step.clone(), report.clone(), win.clone(), shots.clone(), progress.clone(), status.clone(), job.clone(), clicks.clone(), name.clone());
    app::add_timeout3(0.6, move |_| {
        let c = hyprland::own_clients(Some("omarchy-fltk-itest")).into_iter().next();
        if let Some(c) = &c {
            rp.borrow_mut().push(format!("\"hyprland\":{{\"workspace\":\"{}\",\"floating\":{},\"size\":[{},{}]}}", c.workspace, c.floating, c.size.0, c.size.1));
        }
        field.type_text("Omarchy");
        for _ in 0..3 { add.click(); }
        st(format!("\"step\":\"typed+clicked\",\"greeting\":\"{}\",\"state\":\"{}\",\"clicks\":{}", greeting.text(), nm.get(), cl.get()));
        // Keyboard path in a mapped window: first control, Tab, Enter.
        focus::first(&*w);
        focus::press("Tab");
        let add_focused = focus::is_focused(&*add);
        focus::press("Enter");
        st(format!("\"step\":\"keyboard\",\"add_focused\":{add_focused},\"clicks\":{}", cl.get()));
        w.snapshot(format!("{sh}/1-interaction.png")).unwrap();
        stt.set("Working".into());
        let (pr2, stt2, st2) = (pr.clone(), stt.clone(), st.clone());
        let h = Task::new(|ctx| { for i in 0..100 { ctx.sleep(std::time::Duration::from_millis(30))?; ctx.progress((i + 1) as f64 / 100.0); } Ok(()) })
            .on_progress(move |p| pr2.set(p))
            .on_done(move |o: Outcome<()>| { stt2.set(format!("{o:?}")); st2(format!("\"step\":\"task-finished\",\"outcome\":\"{o:?}\"")); })
            .start();
        *jb.borrow_mut() = Some(h);
    });
    let (st, jb, pr) = (step.clone(), job.clone(), progress.clone());
    app::add_timeout3(1.5, move |_| {
        if let Some(h) = jb.borrow().as_ref() { h.cancel(); }
        st(format!("\"step\":\"cancel-requested\",\"progress\":{:.2},\"workers\":{}", pr.get(), task::worker_count()));
    });
    let (st, mut w, sh) = (step.clone(), win.clone(), shots.clone());
    app::add_timeout3(2.0, move |_| {
        w.snapshot(format!("{sh}/2-cancelled.png")).unwrap();
        let mut light = theme::current();
        light.palette.background = theme::Rgb(0xeff1f5);
        light.palette.foreground = theme::Rgb(0x4c4f69);
        light.palette.accent = theme::Rgb(0x1e66f5);
        theme::set(light);
        w.snapshot(format!("{sh}/3-theme-switched.png")).unwrap();
        st("\"step\":\"theme-switched\"".into());
        theme::reload();
        st(format!("\"step\":\"theme-reloaded\",\"accent\":\"{}\"", theme::current().palette.accent.hex()));
    });
    let (st, mut w) = (step.clone(), win.clone());
    app::add_timeout3(2.5, move |_| {
        w.set_mode(Mode::Floating);
        st("\"step\":\"floating-requested\"".into());
    });
    let (st, rp, mut w, sh) = (step.clone(), report.clone(), win.clone(), shots.clone());
    let idle0: Rc<RefCell<(u64, f64)>> = Rc::default();
    let i0 = idle0.clone();
    app::add_timeout3(3.3, move |_| {
        let c = hyprland::own_clients(Some("omarchy-fltk-itest")).into_iter().next();
        let size = c.as_ref().map(|c| format!("[{},{}]", c.size.0, c.size.1)).unwrap_or_else(|| "null".into());
        st(format!("\"step\":\"floating-state\",\"floating\":{},\"size\":{size}", c.as_ref().is_some_and(|c| c.floating)));
        w.snapshot(format!("{sh}/4-floating.png")).unwrap();
        *i0.borrow_mut() = (pss_kb(), cpu_s());
        let _ = &rp;
    });
    let (rp, mut w) = (report.clone(), win.clone());
    app::add_timeout3(6.3, move |_| {
        let (p0, c0) = *idle0.borrow();
        rp.borrow_mut().push(format!("\"idle_3s\":{{\"pss_kb_before\":{p0},\"pss_kb_after\":{},\"cpu_s\":{:.4}}}", pss_kb(), cpu_s() - c0));
        w.hide();
    });
    app.run(); // returns when the window is hidden; cancels tasks, joins workers

    let threads = std::fs::read_dir("/proc/self/task").map(|d| d.count()).unwrap_or(0);
    let mut out = report.borrow().clone();
    out.push(format!("\"after_run\":{{\"os_threads\":{threads},\"workers\":{}}}", task::worker_count()));
    out.push(format!("\"steps\":[{}]", log.borrow().join(",")));
    std::fs::write(&report_path, format!("{{{}}}\n", out.join(","))).unwrap();
}
