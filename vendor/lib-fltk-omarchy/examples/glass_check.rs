//! Glassy windows check in a real window: paints glass surfaces and opaque
//! text, reads the pixels FLTK hands to the compositor (the window's Cairo
//! buffer) and writes a JSON report. Then turns glass off and checks the
//! same window is opaque again.
//!
//!     OMARCHY_FLTK_GLASS=on cargo run --example glass_check -- report.json
//!
//! Linux (native Wayland) diagnostic: the buffer is read through FLTK's
//! Wayland Cairo context. On other OSes it builds, and reports an empty
//! buffer (glass is opaque there, docs/PLATFORMS.md).
use fltk::{enums::*, frame::Frame, prelude::*, window::Window};
use fltk_omarchy::{glass, theme::Rgb};
use std::cell::RefCell;
#[cfg(target_os = "linux")]
use std::ffi::{c_int, c_void};
use std::rc::Rc;

#[cfg(target_os = "linux")]
extern "C" {
    #[link_name = "_Z8fl_wl_gcv"]
    fn fl_wl_gc() -> *mut c_void;
    fn cairo_get_target(cr: *mut c_void) -> *mut c_void;
    fn cairo_surface_flush(s: *mut c_void);
    fn cairo_image_surface_get_data(s: *mut c_void) -> *mut u8;
    fn cairo_image_surface_get_stride(s: *mut c_void) -> c_int;
    fn cairo_image_surface_get_width(s: *mut c_void) -> c_int;
    fn cairo_image_surface_get_height(s: *mut c_void) -> c_int;
}

#[derive(Default, Clone, Debug)]
struct Frame1 {
    size: (i32, i32),
    background: [u8; 4],
    card: [u8; 4],
    text_max_alpha: u8,
    text_min_alpha: u8,
}

#[cfg(not(target_os = "linux"))]
fn read(_points: &[(f64, f64)], _text: (f64, f64, f64, f64)) -> Frame1 {
    Frame1::default()
}

/// BGRA (native-endian ARGB32) of the current window buffer.
#[cfg(target_os = "linux")]
fn read(points: &[(f64, f64)], text: (f64, f64, f64, f64)) -> Frame1 {
    unsafe {
        let cr = fl_wl_gc();
        if cr.is_null() {
            return Frame1::default();
        }
        let s = cairo_get_target(cr);
        cairo_surface_flush(s);
        let (w, h, stride) = (cairo_image_surface_get_width(s), cairo_image_surface_get_height(s), cairo_image_surface_get_stride(s));
        let data = std::slice::from_raw_parts(cairo_image_surface_get_data(s), (stride * h) as usize);
        let px = |fx: f64, fy: f64| {
            let (x, y) = (((fx * w as f64) as i32).clamp(0, w - 1), ((fy * h as f64) as i32).clamp(0, h - 1));
            let o = (y * stride + x * 4) as usize;
            [data[o], data[o + 1], data[o + 2], data[o + 3]]
        };
        let mut f = Frame1 { size: (w, h), background: px(points[0].0, points[0].1), card: px(points[1].0, points[1].1), text_max_alpha: 0, text_min_alpha: 255 };
        let (x0, y0, x1, y1) = text;
        for y in ((y0 * h as f64) as i32)..((y1 * h as f64) as i32) {
            for x in ((x0 * w as f64) as i32)..((x1 * w as f64) as i32) {
                let a = data[(y * stride + x * 4 + 3) as usize];
                f.text_max_alpha = f.text_max_alpha.max(a);
                f.text_min_alpha = f.text_min_alpha.min(a);
            }
        }
        f
    }
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "glass-check.json".into());
    let app = fltk::app::App::default();
    let mut win = Window::default().with_size(360, 220).with_label("glass check");
    win.set_xclass("fltk-omarchy-glass-check");
    let mut canvas = Frame::new(0, 0, 360, 220, "");
    canvas.set_frame(FrameType::NoBox);
    win.end();
    let frames: Rc<RefCell<Vec<Frame1>>> = Rc::default();
    let seen = frames.clone();
    canvas.draw(move |f| {
        glass::fill_rect(f.x(), f.y(), f.w(), f.h(), Rgb(0x1e1e2e));
        glass::fill(30, 30, 300, 120, 10, Rgb(0x313244));
        fltk::draw::set_draw_color(Color::from_hex(0xcdd6f4));
        fltk::draw::set_font(Font::HelveticaBold, 40);
        fltk::draw::draw_text2("Glass", 40, 50, 280, 80, Align::Center);
        // background corner, card centre-left below the text, text box
        seen.borrow_mut().push(read(&[(0.02, 0.9), (0.15, 0.62)], (0.3, 0.3, 0.7, 0.55)));
    });
    win.show();
    let started = std::time::Instant::now();
    let mut phase = 0;
    let mut switched_at = 0usize;
    let mut report = serde_json::json!({});
    while app.wait() {
        let n = frames.borrow().len();
        if phase == 0 && n >= 1 && started.elapsed().as_millis() > 400 {
            let f = frames.borrow().last().cloned().unwrap();
            report["glass_on"] = serde_json::json!({
                "setting": format!("{:?}", glass::setting()), "opacity": glass::opacity(),
                "size": f.size, "background_bgra": f.background, "card_bgra": f.card,
                "text_alpha_max": f.text_max_alpha, "text_alpha_min": f.text_min_alpha,
                "stats": format!("{:?}", glass::stats()),
            });
            glass::set(glass::Setting { mode: glass::Mode::Opaque, opacity: 1.0, overridden: true });
            switched_at = n;
            phase = 1;
        } else if phase == 1 && n > switched_at && started.elapsed().as_millis() > 900 {
            let f = frames.borrow().last().cloned().unwrap();
            report["glass_off"] = serde_json::json!({
                "background_bgra": f.background, "card_bgra": f.card,
                "stats": format!("{:?}", glass::stats()),
            });
            let on = &report["glass_on"];
            let a = glass::SYSTEM_GLASS_OPACITY;
            let expect = (a * 255.0).round() as i64;
            let near = |v: &serde_json::Value, e: i64| v.as_i64().is_some_and(|x| (x - e).abs() <= 1);
            let pass = glass::wayland()
                && near(&on["background_bgra"][3], expect)
                && near(&on["card_bgra"][3], expect)
                && on["text_alpha_max"] == 255
                && f.background[3] == 255
                && f.card[3] == 255
                && glass::stats().regions_cleared == 1
                && glass::stats().regions_restored == 1;
            report["wayland"] = glass::wayland().into();
            report["expected_alpha"] = expect.into();
            report["pass"] = pass.into();
            std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap()).unwrap();
            println!("{}", if pass { "GLASS_CHECK PASS" } else { "GLASS_CHECK FAIL" });
            break;
        }
        if started.elapsed().as_secs() > 8 {
            std::fs::write(&out, serde_json::to_string_pretty(&report).unwrap()).unwrap();
            println!("GLASS_CHECK TIMEOUT");
            break;
        }
        fltk::app::sleep(0.02);
    }
}
