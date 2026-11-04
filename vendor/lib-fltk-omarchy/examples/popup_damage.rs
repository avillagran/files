//! Regression for overlays on a plain FLTK window with a custom canvas.
//! Uses the mapped framebuffer: an offscreen full redraw hides this bug.
//! Run under Xvfb (FLTK_BACKEND=x11) or on an isolated Wayland workspace.
use fltk::{app, draw, enums::Color, frame::Frame, prelude::*, window::Window};
use fltk_omarchy::{
    popups::{PopupItem, PopupMenu},
};
use std::time::{Duration, Instant};
use std::{cell::Cell, rc::Rc};

fn settle() {
    let until = Instant::now() + Duration::from_millis(100);
    while Instant::now() < until {
        app::wait_for(0.01).unwrap();
    }
    app::flush();
}

fn main() {
    // Files initializes plain FLTK and its theme, not App::new/focus::install.
    let _app = app::App::default();
    fltk_omarchy::theme::set(fltk_omarchy::theme::Theme::load());
    let mut win = Window::default()
        .with_size(480, 320)
        .with_label("popup-damage");
    let mut canvas = Frame::new(0, 0, 480, 320, "");
    let draws = Rc::new(Cell::new(0));
    let count = draws.clone();
    canvas.draw(move |w| {
        count.set(count.get() + 1);
        draw::set_draw_color(Color::from_rgb(17, 97, 47));
        draw::draw_rectf(w.x(), w.y(), w.w(), w.h());
    });
    win.end();
    win.resizable(&canvas);
    win.show();
    settle();
    let menu = PopupMenu::open_at(
        &canvas,
        100,
        100,
        vec![PopupItem::new("", "First"), PopupItem::new("", "Second")],
        |_| {},
    );
    settle();
    let (x, y, _, _) = menu.rect().unwrap();
    win.make_current();
    let before = draw::capture_window(&mut win).unwrap().to_rgb_data();
    // Dispatch to the installed popup handler, without OS input injection.
    if let Some(mut layer) = app::focus() {
        layer.handle_event(fltk::enums::Event::Released);
    }
    // Files redraws its canvas when the opening mouse button is released.
    canvas.redraw();
    settle();
    win.make_current();
    let after = draw::capture_window(&mut win).unwrap().to_rgb_data();
    let width = win.w();
    let sample = |data: &[u8]| {
        let start = (((y + 10) * width + x + 10) * 3) as usize;
        data[start..start + 3].to_vec()
    };
    let (a, b) = (sample(&before), sample(&after));
    println!("open={} before={a:?} after={b:?}", menu.is_open());
    let preserved = menu.is_open() && a == b && b != [17, 97, 47];
    canvas.redraw();
    fltk_omarchy::app::flush();
    win.make_current();
    let explicit = draw::capture_window(&mut win).unwrap().to_rgb_data();
    assert_eq!(sample(&explicit), a, "explicit toolkit flush preserves overlays");
    let idle = draws.get();
    settle();
    assert_eq!(draws.get(), idle, "an idle popup must not keep repainting");
    // Repeated canvas updates must not dismiss the menu or strand old layers.
    for _ in 0..5 {
        canvas.redraw();
        settle();
        assert!(menu.is_open());
    }
    menu.close();
    settle();
    win.make_current();
    let closed = draw::capture_window(&mut win).unwrap().to_rgb_data();
    assert_eq!(sample(&closed), [17, 97, 47], "closing restores the canvas");
    win.hide();
    assert!(preserved, "canvas redraw covered an open popup");
}
