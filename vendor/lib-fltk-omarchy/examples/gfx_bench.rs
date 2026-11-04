//! Cost of a CPU shader: one 520×400 window with a 400×300 View, either
//! static (rendered once) or animated at 30 fps. Run by script/gfx-bench on
//! a hidden Hyprland workspace. Prints one JSON line:
//!   gfx_bench <static|animated> <seconds> <report.json> [noise|gradient] [max_area] [threads]
use fltk::prelude::*;
use fltk_omarchy::gfx::{self, View};
use fltk_omarchy::{App, AppWindow};
use std::cell::RefCell;
use std::rc::Rc;

fn pss_tree_kb() -> u64 {
    // This process has no children: its PSS + SwapPss is the tree's.
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
    let a: Vec<String> = std::env::args().collect();
    let mode = a.get(1).cloned().unwrap_or_else(|| "static".into());
    let secs: f64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(5.0);
    let out = a.get(3).cloned().unwrap_or_else(|| "build/gfx-bench.json".into());
    let shader = a.get(4).cloned().unwrap_or_else(|| "noise".into());
    let max_area: usize = a.get(5).and_then(|s| s.parse().ok()).unwrap_or(160_000);
    let threads: usize = a.get(6).and_then(|s| s.parse().ok()).unwrap_or(1);
    let app = App::without_theme_watch("dev.omarchy.fltk.gfxbench");
    let mut win = AppWindow::new("gfx-bench", 520, 400);
    let mut body = win.body();
    let view = View::new(400, 300).max_area(max_area).threads(threads);
    let mut view = if shader == "gradient" { view.shader(gfx::shaders::gradient()) } else { view.shader(gfx::shaders::noise()) };
    if mode == "animated" {
        view = view.animate(30);
    }
    fltk_omarchy::layout::FlexExt::fit(&mut body, &*view);
    win.end();
    win.set_mode(fltk_omarchy::Mode::Floating);
    win.show();
    let sample: Rc<RefCell<Option<(f64, u64, std::time::Instant)>>> = Rc::default();
    let (s2, v2) = (sample.clone(), view.clone());
    fltk::app::add_timeout3(1.0, move |_| { *s2.borrow_mut() = Some((cpu_s(), v2.stats().frames, std::time::Instant::now())); });
    let (s3, v3, m, sh) = (sample.clone(), view.clone(), mode.clone(), shader.clone());
    let mut w = win.clone();
    fltk::app::add_timeout3(1.0 + secs, move |_| {
        let (c0, f0, t0) = s3.borrow().expect("sample");
        let (cpu, frames, wall) = (cpu_s() - c0, v3.stats().frames - f0, t0.elapsed().as_secs_f64());
        let st = v3.stats();
        let line = format!(
            "{{\"mode\":\"{m}\",\"shader\":\"{sh}\",\"max_area\":{max_area},\"threads\":{threads},\"seconds\":{wall:.2},\"frames\":{frames},\"fps\":{:.1},\"cpu_s\":{cpu:.3},\"cpu_pct\":{:.1},\"cpu_ms_per_frame\":{},\"last_frame_ms\":{:.2},\"last_blit_ms\":{:.2},\"partial_draws\":{},\"render_size\":[{},{}],\"view_buffers_kb\":{},\"pss_tree_kb\":{}}}",
            frames as f64 / wall, 100.0 * cpu / wall,
            if frames > 0 { format!("{:.2}", cpu * 1000.0 / frames as f64) } else { "null".into() },
            st.last_frame_ms, st.last_blit_ms, st.partial_draws, st.render_size.0, st.render_size.1, st.memory / 1024, pss_tree_kb());
        std::fs::write(&out, format!("{line}\n")).unwrap();
        w.hide();
    });
    app.run();
}
