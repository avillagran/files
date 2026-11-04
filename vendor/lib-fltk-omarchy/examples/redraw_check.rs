//! Partial redraws in a real window: redraw one button and check that the
//! others are still drawn (reads the window back). Run on a hidden workspace.
//!   redraw_check <report>
use fltk::prelude::*;
use fltk_omarchy::layout::{self, FlexExt};
use fltk_omarchy::widgets::*;
use fltk_omarchy::{App, AppWindow};

fn main() {
    let out = std::env::args().nth(1).unwrap();
    let app = App::without_theme_watch("dev.omarchy.fltk.redraw");
    let mut win = AppWindow::new("redraw-check", 420, 200);
    let mut r = layout::row();
    let a = Button::new("Alpha").bordered();
    r.fit(&*a);
    let b = Button::new("Beta").bordered();
    r.fit(&*b);
    r.close_fitted();
    win.end();
    win.show();
    let sample = |w: &mut fltk::window::DoubleWindow, x: i32, y: i32| -> String {
        match fltk::draw::capture_window(w) {
            Ok(img) => {
                let d = img.to_rgb_data();
                let (iw, ih) = (img.data_w(), img.data_h());
                let sx = iw as f64 / w.w() as f64;
                let (px, py) = ((x as f64 * sx) as i32, (y as f64 * sx) as i32);
                let i = ((py * iw + px) * img.depth() as i32) as usize;
                let _ = ih;
                format!("[{},{},{}]", d[i], d[i + 1], d[i + 2])
            }
            Err(e) => format!("\"{e:?}\""),
        }
    };
    let (mut w, a2, b2) = ((*win).clone(), a.clone(), b.clone());
    let mut log = Vec::new();
    fltk::app::add_timeout3(1.0, move |_| {
        // Border pixel of Beta (left edge, middle) before and after redrawing Alpha only.
        let (bx, by) = (b2.x(), b2.y() + b2.h() / 2);
        log.push(format!("\"beta_before\":{}", sample(&mut w, bx, by)));
        a2.clone().redraw();
        fltk::app::flush();
        log.push(format!("\"beta_after_alpha_redraw\":{}", sample(&mut w, bx, by)));
        log.push(format!("\"alpha_after\":{}", sample(&mut w, a2.x(), a2.y() + a2.h() / 2)));
        // A menu layer over the whole window, redrawn alone (hover): the button behind must stay.
        let h = fltk_omarchy::popups::PopupMenu::open(&*b2, vec![fltk_omarchy::popups::PopupItem::new("", "One"), fltk_omarchy::popups::PopupItem::new("", "Two")], |_| {});
        fltk::app::flush();
        log.push(format!("\"alpha_with_menu\":{}", sample(&mut w, a2.x(), a2.y() + a2.h() / 2)));
        let top = w.child(w.children() - 1).unwrap();
        top.clone().redraw();
        fltk::app::flush();
        log.push(format!("\"alpha_after_menu_redraw\":{}", sample(&mut w, a2.x(), a2.y() + a2.h() / 2)));
        let (mx, my, _, _) = h.rect().unwrap();
        log.push(format!("\"menu_border_after\":{}", sample(&mut w, mx, my + 10)));
        h.close();
        std::fs::write(&out, format!("{{{}}}\n", log.join(","))).unwrap();
        w.hide();
    });
    app.run();
}
