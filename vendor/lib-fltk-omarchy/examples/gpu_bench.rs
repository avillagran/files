//! GPU shader check and cost (needs `--features gpu`). Run by
//! script/gpu-bench on a hidden Hyprland workspace:
//!   gpu_bench check <report.json>          pixel check + compile error check
//!   gpu_bench <static|animated> <seconds> <report.json>
use fltk::prelude::*;
use fltk_omarchy::gfx::gpu::GpuView;
use fltk_omarchy::{App, AppWindow};
use std::cell::RefCell;
use std::rc::Rc;

const NOISE: &str = r#"
float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
float noise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1, 0)), u.x), mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), u.x), u.y);
}
vec4 shade(vec2 p) {
    vec2 q = p / 48.0 + vec2(u_time * 0.15, -u_time * 0.1);
    float n = 0.5 * noise(q) + 0.25 * noise(q * 2.0) + 0.125 * noise(q * 4.0);
    return mix(u_background, u_foreground, n / 0.875 * 0.35);
}
"#;

fn pss_tree_kb() -> u64 {
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
    let mode = a.get(1).cloned().unwrap_or_else(|| "check".into());
    let app = App::without_theme_watch("dev.omarchy.fltk.gpubench");
    let mut win = AppWindow::new("gpu-bench", 520, 400);
    let mut body = win.body();
    if mode == "check" {
        let out = a.get(2).cloned().unwrap_or_else(|| "build/gpu-check.json".into());
        let solid = GpuView::new(200, 120, "vec4 shade(vec2 p) { return p.x < u_size.x * 0.5 ? u_accent : u_background; }");
        let broken = GpuView::new(100, 60, "vec4 shade(vec2 p) { return nope; }");
        fltk_omarchy::layout::FlexExt::fit(&mut body, &*solid);
        fltk_omarchy::layout::FlexExt::fit(&mut body, &*broken);
        win.end();
        win.set_mode(fltk_omarchy::Mode::Floating);
        win.show();
        let acc = fltk_omarchy::theme::current().palette.accent;
        let bg = fltk_omarchy::theme::current().palette.background;
        let result: Rc<RefCell<Vec<String>>> = Rc::default();
        let (r1, s1) = (result.clone(), solid.clone());
        fltk::app::add_timeout3(0.8, move |_| {
            let (r2, r3) = (r1.clone(), r1.clone());
            let right_x = s1.w() - 10;
            s1.read_pixel(10, 60, move |px| r2.borrow_mut().push(format!("\"left\":[{},{},{}]", px[0], px[1], px[2])));
            s1.read_pixel(right_x, 60, move |px| r3.borrow_mut().push(format!("\"right\":[{},{},{}]", px[0], px[1], px[2])));
        });
        let (r4, b4, mut w) = (result.clone(), broken.clone(), win.clone());
        let s4 = solid.clone();
        fltk::app::add_timeout3(1.6, move |_| {
            let mut r = r4.borrow_mut();
            r.push(format!("\"accent\":[{},{},{}],\"background\":[{},{},{}]", acc.r(), acc.g(), acc.b(), bg.r(), bg.g(), bg.b()));
            r.push(format!("\"compile_error\":{}", serde_json::to_string(&b4.last_error()).unwrap()));
            r.push(format!("\"solid_error\":{}", serde_json::to_string(&s4.last_error()).unwrap()));
            r.push(format!("\"stats\":\"{:?}\",\"live\":{}", s4.stats(), s4.is_live()));
            std::fs::write(&out, format!("{{{}}}\n", r.join(","))).unwrap();
            w.hide();
        });
        app.run();
        return;
    }
    if mode == "demo" {
        // A few GPU shaders side by side for a screenshot (script captures it).
        let mut row = fltk_omarchy::layout::row();
        let glow = GpuView::new(160, 300, "vec4 shade(vec2 p) { float d = length(p - u_size * 0.5) / min(u_size.x, u_size.y); return mix(u_accent, u_background, smoothstep(0.0, 0.5 + 0.05 * sin(u_time * 2.0), d)); }").animate(30);
        let noise = GpuView::new(160, 300, NOISE).animate(30);
        let rings = GpuView::new(160, 300, "vec4 shade(vec2 p) { float d = length(p - u_size * 0.5); return mix(u_background, u_accent, (0.5 + 0.5 * sin(d * 0.15 - u_time * 3.0)) * 0.6); }").animate(30);
        for v in [&glow, &noise, &rings] { fltk_omarchy::layout::FlexExt::fit(&mut row, &**v); }
        fltk_omarchy::layout::FlexExt::close_fitted(&mut row);
        win.end();
        win.set_mode(fltk_omarchy::Mode::Floating);
        win.show();
        let mut w = win.clone();
        fltk::app::add_timeout3(a.get(2).and_then(|s| s.parse().ok()).unwrap_or(4.0), move |_| w.hide());
        app.run();
        return;
    }
    if mode == "parity" {
        // Same portable shader on the GPU (read back) and on the CPU (sl eval);
        // a paint() rectangle on top must show through the overlay.
        use fltk_omarchy::gfx::{self, Path, View};
        let out = a.get(2).cloned().unwrap_or_else(|| "build/gpu-parity.json".into());
        let sh = gfx::shaders::gradient();
        let view = View::new(400, 300).shader(sh.clone()).paint(|c, u| c.fill(&Path::rect(0.0, 0.0, 30.0, 30.0), u.accent));
        view.set_time(Some(0.7));
        fltk_omarchy::layout::FlexExt::fit(&mut body, &*view);
        win.end();
        win.set_mode(fltk_omarchy::Mode::Floating);
        win.show();
        let res: Rc<RefCell<Vec<String>>> = Rc::default();
        let (v2, r2, sh2) = (view.clone(), res.clone(), sh.clone());
        fltk::app::add_timeout3(1.6, move |_| {
            r2.borrow_mut().push(format!("\"backend\":\"{:?}\",\"reason\":{:?}", v2.active_backend(), v2.backend_reason()));
            let mut u = gfx::Uniforms::from_theme(&fltk_omarchy::theme::current(), (v2.w() as f32, v2.h() as f32));
            u.time = 0.7;
            for (x, y) in [(15, 15), (60, 40), (200, 150), (390, 290)] {
                let want = sh2.eval(x as f32 + 0.5, y as f32 + 0.5, &u).to_u8();
                let r3 = r2.clone();
                let read = v2.read_gpu_pixel(x, y, move |px| r3.borrow_mut().push(format!("\"p{x}_{y}\":{{\"gpu\":[{},{},{}],\"cpu\":[{},{},{}]}}", px[0], px[1], px[2], want[0], want[1], want[2])));
                if !read { r2.borrow_mut().push(format!("\"p{x}_{y}\":null")); }
            }
        });
        let (r4, mut w) = (res.clone(), win.clone());
        let acc = fltk_omarchy::theme::current().palette.accent;
        fltk::app::add_timeout3(2.4, move |_| {
            r4.borrow_mut().push(format!("\"accent\":[{},{},{}]", acc.r(), acc.g(), acc.b()));
            std::fs::write(&out, format!("{{{}}}\n", r4.borrow().join(","))).unwrap();
            w.hide();
        });
        app.run();
        return;
    }
    if mode == "display" {
        // Rate::Display: frames per second actually drawn, by backend.
        use fltk_omarchy::gfx::{self, Backend, Rate, View};
        let secs: f64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(5.0);
        let out = a.get(3).cloned().unwrap_or_else(|| "build/gpu-display.json".into());
        let backend = if a.get(4).map(String::as_str) == Some("cpu") { Backend::Cpu } else { Backend::Auto };
        let view = View::new(400, 300).shader(gfx::shaders::noise()).animate(Rate::Display).backend(backend);
        fltk_omarchy::layout::FlexExt::fit(&mut body, &*view);
        win.end();
        win.set_mode(fltk_omarchy::Mode::Floating);
        win.show();
        let sample: Rc<RefCell<Option<(f64, u64, std::time::Instant)>>> = Rc::default();
        let (s2, v2) = (sample.clone(), view.clone());
        fltk::app::add_timeout3(1.5, move |_| { *s2.borrow_mut() = Some((cpu_s(), v2.stats().frames, std::time::Instant::now())); });
        let (s3, v3, mut w) = (sample.clone(), view.clone(), win.clone());
        fltk::app::add_timeout3(1.5 + secs, move |_| {
            let (c0, f0, t0) = s3.borrow().expect("sample");
            let (cpu, frames, wall) = (cpu_s() - c0, v3.stats().frames - f0, t0.elapsed().as_secs_f64());
            let st = v3.stats();
            let line = format!(
                "{{\"mode\":\"display\",\"backend\":\"{:?}\",\"target_fps\":{:.1},\"fps\":{:.1},\"cpu_pct\":{:.1},\"cpu_ms_per_frame\":{:.2},\"pss_tree_kb\":{},\"reason\":{:?}}}",
                v3.active_backend(), st.target_fps, frames as f64 / wall, 100.0 * cpu / wall, if frames > 0 { cpu * 1000.0 / frames as f64 } else { 0.0 }, pss_tree_kb(), v3.backend_reason());
            std::fs::write(&out, format!("{line}\n")).unwrap();
            w.hide();
        });
        app.run();
        return;
    }
    let secs: f64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(5.0);
    let out = a.get(3).cloned().unwrap_or_else(|| "build/gpu-bench.json".into());
    let mut view = GpuView::new(400, 300, NOISE);
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
    let (s3, v3, m, mut w) = (sample.clone(), view.clone(), mode.clone(), win.clone());
    fltk::app::add_timeout3(1.0 + secs, move |_| {
        let (c0, f0, t0) = s3.borrow().expect("sample");
        let (cpu, frames, wall) = (cpu_s() - c0, v3.stats().frames - f0, t0.elapsed().as_secs_f64());
        let line = format!(
            "{{\"backend\":\"gpu\",\"mode\":\"{m}\",\"seconds\":{wall:.2},\"frames\":{frames},\"fps\":{:.1},\"cpu_pct\":{:.1},\"cpu_ms_per_frame\":{},\"last_draw_ms\":{:.2},\"error\":{},\"pss_tree_kb\":{}}}",
            frames as f64 / wall, 100.0 * cpu / wall,
            if frames > 0 { format!("{:.2}", cpu * 1000.0 / frames as f64) } else { "null".into() },
            v3.stats().last_draw_ms, serde_json::to_string(&v3.last_error()).unwrap(), pss_tree_kb());
        std::fs::write(&out, format!("{line}\n")).unwrap();
        w.hide();
    });
    app.run();
}
