//! Isolated toolkit keyboard fixture, not the Files window or native input.
use fltk_omarchy::{App, AppWindow, State, focus, layout::FlexExt, widgets::ButtonGroup};
use std::path::PathBuf;

fn main() {
    let output = PathBuf::from(std::env::args_os().nth(1).expect("output PNG required"));
    let _app = App::without_theme_watch("dev.files.focus-probe");
    let mut window = AppWindow::new("Prueba de foco", 600, 200);
    let mut body = window.body();
    let selected = State::new(0usize);
    let group = ButtonGroup::new(&["Grupos", "Lista", "Grilla"], &selected);
    body.fit(&*group);
    window.end();
    assert!(focus::first(&*window).is_some());
    assert!(focus::press("Right"));
    assert_eq!(selected.get(), 1);
    assert!(focus::show_ring(
        &*group.button(1).expect("selected button")
    ));
    window.snapshot(&output).expect("write PNG");
    println!("focus ring fixture: {}", output.display());
}
