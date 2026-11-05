//! UI checks of the toolkit's tooltips (`fltk_omarchy::tooltip`): hover
//! through `focus::hover_at` (fltk's normal event path), pixels from
//! offscreen snapshots that include the in-window tooltip surface.
use super::{check, png_rgb};
use fltk::enums::{Color, Event};
use fltk::prelude::*;
use fltk_omarchy::focus;
use fltk_omarchy::layout::{self, FlexExt};
use fltk_omarchy::overlay::ConfirmDialog;
use fltk_omarchy::theme::{self, Corners, Rgb};
use fltk_omarchy::tooltip::{self, BorderMode, Settings, Shape};
use fltk_omarchy::widgets::*;
use fltk_omarchy::{glass, i18n, App, AppWindow, State};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

type Check<'a> = &'a dyn Fn(&Path) -> Result<(), String>;

const BLACK: &str =
    "accent = \"#ffffff\"\nbackground = \"#000000\"\nforeground = \"#dddddd\"\nred = \"#ff4444\"\n";

fn pump_for(d: Duration) {
    let end = Instant::now() + d;
    while Instant::now() < end {
        App::pump();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn pump_until_shown(text: &str) -> Option<tooltip::Shown> {
    let end = Instant::now() + Duration::from_secs(2);
    loop {
        App::pump();
        if let Some(s) = tooltip::shown().filter(|s| s.text == text) {
            return Some(s);
        }
        if Instant::now() > end {
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Lays the window out as if shown (offscreen windows are laid out by a
/// snapshot).
fn lay_out(win: &mut AppWindow, shots: &Path) {
    let _ = win.snapshot(shots.join("tooltip-layout.png"));
}

fn center<W: WidgetExt>(w: &W) -> (i32, i32) {
    (w.x() + w.w() / 2, w.y() + w.h() / 2)
}

fn rgb(c: Rgb) -> [u8; 3] {
    [c.r(), c.g(), c.b()]
}

/// The tooltip colors the theme asks for: (fill, border).
fn expected() -> ([u8; 3], [u8; 3]) {
    theme::with(|t| {
        let fill = t.tooltip.fill(t.palette.background);
        (
            rgb(fill),
            rgb(t.tooltip.border.over(fill, t.tooltip.border_alpha)),
        )
    })
}

/// Fill (inside, clear of text) and the border at the middle of the left
/// edge of the shown tooltip in a snapshot.
fn tip_pixels(
    win: &mut AppWindow,
    path: &Path,
) -> Result<(tooltip::Shown, [u8; 3], [u8; 3]), String> {
    let s = tooltip::shown().ok_or("no tooltip shown")?;
    win.snapshot(path).map_err(|e| e.to_string())?;
    let img = png_rgb(path)?;
    let (x, y, _, h) = s.rect;
    // Top padding row, 3 px in from the left border: fill, never text.
    let fill = img.at(x + 3, y + 3);
    let border = img.at(x, y + h / 2);
    Ok((s, fill, border))
}

fn styled(win: &mut AppWindow, path: &Path, what: &str) -> Result<tooltip::Shown, String> {
    let (s, fill, border) = tip_pixels(win, path)?;
    let (want_fill, want_border) = expected();
    check(
        fill == want_fill,
        &format!("{what}: fill {fill:?}, theme says {want_fill:?}"),
    )?;
    check(
        border == want_border,
        &format!("{what}: border {border:?}, theme says {want_border:?}"),
    )?;
    Ok(s)
}

fn write_colors(colors: &str) {
    // Like omarchy-theme-set: a new directory renamed over `theme`.
    let state = PathBuf::from(std::env::var("OMARCHY_FLTK_STATE_DIR").unwrap());
    let next = state.join("next-tooltip-theme");
    let _ = std::fs::remove_dir_all(&next);
    std::fs::create_dir_all(&next).unwrap();
    std::fs::write(next.join("colors.toml"), colors).unwrap();
    let _ = std::fs::remove_dir_all(state.join("theme"));
    std::fs::rename(&next, state.join("theme")).unwrap();
}

fn wait_theme(bg: u32) -> Result<(), String> {
    let end = Instant::now() + Duration::from_secs(3);
    while theme::current().palette.background != Rgb(bg) && Instant::now() < end {
        App::pump();
        std::thread::sleep(Duration::from_millis(10));
    }
    check(
        theme::current().palette.background == Rgb(bg),
        &format!("theme with background {bg:06x} applied"),
    )
}

pub fn checks(run: &mut dyn FnMut(&str, Check), colors_a: &'static str, colors_b: &'static str) {
    run(
        "tooltip: hover shows the Omarchy tooltip after 400 ms, styled by the theme",
        &|shots| {
            check(
                tooltip::installed() && !fltk::misc::Tooltip::enabled(),
                "toolkit tooltips replace FLTK's window",
            )?;
            let mut win = AppWindow::new("t", 420, 220);
            let mut body = win.body();
            let b = Button::new("Save").bordered().tooltip("Save the file");
            body.fit(&*b);
            win.end();
            lay_out(&mut win, shots);
            let (x, y) = center(&*b);
            focus::hover_at(&*win, x, y);
            App::pump();
            check(tooltip::shown().is_none(), "not before the delay")?;
            let t0 = Instant::now();
            let s = pump_until_shown("Save the file").ok_or("never shown")?;
            check(
                t0.elapsed() >= Duration::from_millis(350),
                &format!("shown after {:?}", t0.elapsed()),
            )?;
            let s2 = styled(&mut win, &shots.join("tooltip-hover.png"), "hover")?;
            check(s == s2, "stable")?;
            let (tx, ty, tw, th) = s.rect;
            let sm = theme::with(|t| t.spacing.sm);
            // Centered on the button; above it, or below when the button is at the top.
            check(
                ((tx + tw / 2) - (b.x() + b.w() / 2)).abs() <= 1 || tx == sm,
                &format!(
                    "centered: tip {:?} button {}..{}",
                    s.rect,
                    b.x(),
                    b.x() + b.w()
                ),
            )?;
            check(
                ty + th + sm == b.y() || ty == b.y() + b.h() + sm,
                &format!(
                    "next to the button: tip {:?} button {:?}",
                    s.rect,
                    (b.x(), b.y(), b.w(), b.h())
                ),
            )?;
            // bodySmall text with 10/6 padding and the 1 px border.
            let want_h = theme::with(|t| {
                fltk::draw::set_font(fltk::enums::Font::Helvetica, t.font.body_small);
                fltk::draw::height().max(t.font.body_small)
                    + 2 * (t.spacing.control_padding_y + t.tooltip.border_width)
            });
            check(th == want_h, &format!("height {th}, want {want_h}"))?;
            // The pointer leaves: hidden.
            focus::hover_at(&*win, 2, win.h() - 2);
            App::pump();
            check(tooltip::shown().is_none(), "hidden when the pointer leaves")
        },
    );
    run(
        "tooltip: the next one shows at once while one is up; a click hides it",
        &|shots| {
            let mut win = AppWindow::new("t", 420, 220);
            let _body = win.body();
            let mut row = layout::row();
            let a = Button::new("A").bordered().tooltip("First");
            let b = Button::new("B").bordered().tooltip("Second");
            row.fit(&*a);
            row.fit(&*b);
            row.close_fitted();
            win.end();
            lay_out(&mut win, shots);
            let (ax, ay) = center(&*a);
            focus::hover_at(&*win, ax, ay);
            pump_until_shown("First").ok_or("first never shown")?;
            let (bx, by) = center(&*b);
            focus::hover_at(&*win, bx, by);
            App::pump();
            check(
                tooltip::shown().is_some_and(|s| s.text == "Second"),
                "second at once",
            )?;
            // Through a gap with no tooltip and back within 400 ms: still warm.
            focus::hover_at(&*win, 2, win.h() - 2);
            App::pump();
            check(tooltip::shown().is_none(), "hidden over nothing")?;
            focus::hover_at(&*win, ax, ay);
            App::pump();
            check(
                tooltip::shown().is_some_and(|s| s.text == "First"),
                "warm: no delay right after",
            )?;
            styled(&mut win, &shots.join("tooltip-warm.png"), "warm")?;
            focus::click_at(&*win, ax, ay);
            check(tooltip::shown().is_none(), "a click hides it")?;
            focus::hover_at(&*win, ax + 1, ay);
            pump_for(Duration::from_millis(500));
            check(
                tooltip::shown().is_none(),
                "and it stays hidden until the pointer leaves",
            )?;
            focus::hover_at(&*win, 2, win.h() - 2);
            pump_for(Duration::from_millis(450));
            focus::hover_at(&*win, ax, ay);
            pump_until_shown("First").ok_or("shown again after leaving")?;
            tooltip::hide();
            Ok(())
        },
    );
    run(
        "tooltip: live theme switch (omarchy-theme-set), black themes, corners",
        &|shots| {
            let mut win = AppWindow::new("t", 420, 220);
            let mut body = win.body();
            let b = Button::new("Go").bordered().tooltip("Run it");
            body.fit(&*b);
            win.end();
            lay_out(&mut win, shots);
            check(tooltip::show(&*b), "show")?;
            styled(&mut win, &shots.join("tooltip-theme-a.png"), "theme A")?;
            write_colors(colors_b);
            wait_theme(0x1a1b26)?;
            check(
                tooltip::shown().is_some_and(|s| s.text == "Run it"),
                "still shown after the switch",
            )?;
            styled(
                &mut win,
                &shots.join("tooltip-theme-b.png"),
                "theme B (live)",
            )?;
            // A pure black background (vantablack) is black, not the text color.
            write_colors(BLACK);
            wait_theme(0x000000)?;
            check(Rgb(0).fl() == Color::Black, "#000000 is FLTK black")?;
            check(
                fltk::misc::Tooltip::color().to_rgb() == (0, 0, 0),
                &format!(
                    "FLTK tooltip color {:?}",
                    fltk::misc::Tooltip::color().to_rgb()
                ),
            )?;
            let (_, fill, _) = tip_pixels(&mut win, &shots.join("tooltip-black.png"))?;
            check(fill == [0, 0, 0], &format!("black theme: fill {fill:?}"))?;
            styled(&mut win, &shots.join("tooltip-black.png"), "black theme")?;
            write_colors(colors_a);
            wait_theme(0x121212)?;
            // Corners: the tooltip follows set_corners (theme radius).
            theme::set_corners(Corners::Rounded);
            let (s, _, border) = tip_pixels(&mut win, &shots.join("tooltip-rounded.png"))?;
            let img = png_rgb(&shots.join("tooltip-rounded.png"))?;
            let corner = img.at(s.rect.0, s.rect.1);
            check(
                corner != border,
                &format!("rounded: the corner pixel is outside ({corner:?})"),
            )?;
            theme::set_corners(Corners::Square);
            let (s, _, border) = tip_pixels(&mut win, &shots.join("tooltip-square.png"))?;
            let img = png_rgb(&shots.join("tooltip-square.png"))?;
            check(
                img.at(s.rect.0, s.rect.1) == border,
                "square: the corner is border",
            )?;
            theme::set_corners(Corners::Omarchy);
            tooltip::hide();
            Ok(())
        },
    );
    run(
        "tooltip: tooltips.json shape and border are read live; invalid keeps the last",
        &|shots| {
            let path = tooltip::settings_path();
            let mut win = AppWindow::new("t", 420, 220);
            let mut body = win.body();
            let b = Button::new("Go").bordered().tooltip("Settings");
            body.fit(&*b);
            win.end();
            lay_out(&mut win, shots);
            check(tooltip::show(&*b), "show")?;
            std::fs::write(
                &path,
                r##"{"shape":"rounded","borderMode":"custom","borderColor":"#ff0000"}"##,
            )
            .unwrap();
            let want = Settings {
                shape: Shape::Rounded,
                border: BorderMode::Custom(Rgb(0xff0000)),
            };
            let end = Instant::now() + Duration::from_secs(3);
            while tooltip::settings() != want && Instant::now() < end {
                App::pump();
                std::thread::sleep(Duration::from_millis(10));
            }
            check(
                tooltip::settings() == want,
                &format!("watcher picked it up: {:?}", tooltip::settings()),
            )?;
            let (s, _, border) = tip_pixels(&mut win, &shots.join("tooltip-settings.png"))?;
            check(border == [0xff, 0, 0], &format!("custom border {border:?}"))?;
            let img = png_rgb(&shots.join("tooltip-settings.png"))?;
            check(img.at(s.rect.0, s.rect.1) != border, "rounded shape")?;
            std::fs::write(&path, r#"{"shape":"oval"}"#).unwrap();
            check(
                !tooltip::reload_settings() && tooltip::settings() == want,
                "invalid file keeps the last valid settings",
            )?;
            std::fs::write(&path, r#"{"borderMode":"transparent","shape":"square"}"#).unwrap();
            check(tooltip::reload_settings(), "transparent border")?;
            let (_, fill, border) = tip_pixels(&mut win, &shots.join("tooltip-transparent.png"))?;
            check(
                border == fill,
                &format!("no border drawn: {border:?} vs {fill:?}"),
            )?;
            std::fs::remove_file(&path).unwrap();
            check(
                tooltip::reload_settings() && tooltip::settings() == Settings::default(),
                "removed: defaults",
            )?;
            tooltip::hide();
            Ok(())
        },
    );
    run(
        "tooltip: after a dialog, in a second window, plain fltk widgets, glass",
        &|shots| {
            let mut win = AppWindow::new("t", 420, 220);
            let mut body = win.body();
            let b = Button::new("Go").bordered().tooltip("Behind the dialog");
            body.fit(&*b);
            // A plain fltk frame (it takes no FL_ENTER) with its own tooltip.
            let mut f = fltk::frame::Frame::default().with_label("plain");
            f.set_tooltip("Plain fltk");
            body.fit(&f);
            win.end();
            lay_out(&mut win, shots);
            ConfirmDialog::open(&*win, "Sure?", "Yes", false, |_| {});
            let (x, y) = center(&*b);
            focus::hover_at(&*win, x, y);
            pump_for(Duration::from_millis(500));
            check(tooltip::shown().is_none(), "nothing through a modal dialog")?;
            focus::press("Escape");
            focus::hover_at(&*win, 2, win.h() - 2);
            focus::hover_at(&*win, x, y);
            pump_until_shown("Behind the dialog").ok_or("shown after the dialog closed")?;
            styled(
                &mut win,
                &shots.join("tooltip-after-dialog.png"),
                "after a dialog",
            )?;
            let (fx, fy) = center(&f);
            focus::hover_at(&*win, fx, fy);
            pump_until_shown("Plain fltk").ok_or("plain fltk frame")?;
            styled(&mut win, &shots.join("tooltip-plain.png"), "plain fltk")?;
            tooltip::hide();
            // A second window.
            let mut second = AppWindow::new("second", 360, 200);
            let mut body2 = second.body();
            let c = Button::new("Other")
                .bordered()
                .tooltip("In the second window");
            body2.fit(&*c);
            second.end();
            lay_out(&mut second, shots);
            let (cx, cy) = center(&*c);
            focus::hover_at(&*second, cx, cy);
            let s = pump_until_shown("In the second window").ok_or("second window")?;
            check(tip_parent_is(&*second), "drawn in the second window")?;
            styled(
                &mut second,
                &shots.join("tooltip-second-window.png"),
                "second window",
            )?;
            check(
                s.rect.0 >= 0 && s.rect.0 + s.rect.2 <= second.w(),
                "inside the second window",
            )?;
            tooltip::hide();
            // Glass: the tooltip stays an opaque Omarchy surface.
            let before = glass::setting();
            glass::set(glass::Setting {
                mode: glass::Mode::Glassy,
                opacity: 0.65,
                overridden: true,
            });
            check(tooltip::show(&*b), "show with glass")?;
            styled(&mut win, &shots.join("tooltip-glass.png"), "glass")?;
            glass::set(before);
            tooltip::hide();
            Ok(())
        },
    );
    run(
        "tooltip: canvases with one tooltip per region, large widgets, wrapping",
        &|shots| {
            let mut win = AppWindow::new("t", 480, 260);
            let mut body = win.body();
            // Like Files: one canvas that sets its tooltip from the pointer.
            let mut canvas = fltk::widget::Widget::default();
            canvas.handle(|w, ev| {
                if matches!(ev, Event::Enter | Event::Move) {
                    let left = fltk::app::event_x() < w.x() + w.w() / 2;
                    w.set_tooltip(if left { "Left region" } else { "Right region" });
                    return true;
                }
                false
            });
            body.add(&canvas);
            win.end();
            lay_out(&mut win, shots);
            let (x0, y0) = (canvas.x() + 40, canvas.y() + canvas.h() / 2);
            focus::hover_at(&*win, x0, y0);
            let s = pump_until_shown("Left region").ok_or("left region")?;
            let cursor =
                theme::with(|t| (20.0 * t.font.base as f64 / 12.0).round() as i32 + t.spacing.sm);
            check(
                s.rect.1 == y0 + cursor,
                &format!(
                    "below the pointer on a large widget: {:?} pointer y {y0}",
                    s.rect
                ),
            )?;
            check(
                (s.rect.0 + s.rect.2 / 2 - x0).abs() <= 1
                    || s.rect.0 == theme::with(|t| t.spacing.sm),
                &format!("centered on the pointer: {:?} x {x0}", s.rect),
            )?;
            focus::hover_at(&*win, canvas.x() + canvas.w() - 40, y0);
            App::pump();
            check(
                tooltip::shown().is_some_and(|s| s.text == "Right region"),
                "the region's tooltip at once",
            )?;
            styled(&mut win, &shots.join("tooltip-canvas.png"), "canvas region")?;
            tooltip::hide();
            // Long text wraps at 360 px (scaled) and stays inside the window.
            let long = "Copies the selected files to the other pane, keeping their names, dates and permissions, and asks before replacing anything";
            canvas.set_tooltip(long);
            let mut l = Button::new("Copy").bordered().tooltip(long);
            win.body().fit(&*l);
            let _ = &mut l;
            check(tooltip::show(&*l), "show long")?;
            let s = tooltip::shown().ok_or("long shown")?;
            let max = theme::with(|t| (360.0 * t.font.base as f64 / 12.0).round() as i32);
            let one_line = theme::with(|t| {
                fltk::draw::set_font(fltk::enums::Font::Helvetica, t.font.body_small);
                fltk::draw::height().max(t.font.body_small)
                    + 2 * (t.spacing.control_padding_y + t.tooltip.border_width)
            });
            check(
                s.rect.2 <= max && s.rect.3 > one_line,
                &format!("wrapped: {:?} (max width {max})", s.rect),
            )?;
            check(
                s.rect.0 >= 0
                    && s.rect.0 + s.rect.2 <= win.w()
                    && s.rect.1 >= 0
                    && s.rect.1 + s.rect.3 <= win.h(),
                "inside the window",
            )?;
            styled(&mut win, &shots.join("tooltip-wrapped.png"), "wrapped")?;
            tooltip::hide();
            Ok(())
        },
    );
    run(
        "tooltip: bound text relabels a shown tooltip; right to left aligns right",
        &|shots| {
            let label = State::new("Open".to_string());
            let mut win = AppWindow::new("t", 420, 200);
            let mut body = win.body();
            let b =
                ActionButton::new(fltk_omarchy::widgets::glyph::SETTINGS, "").bind_tooltip(&label);
            body.fit(&*b);
            win.end();
            lay_out(&mut win, shots);
            check(tooltip::show(&*b), "show bound")?;
            label.set("Abrir".into());
            App::pump();
            check(
                tooltip::shown().is_some_and(|s| s.text == "Abrir"),
                &format!("relabeled: {:?}", tooltip::shown()),
            )?;
            tooltip::hide();
            // Right to left: the short last line of a wrapped tooltip ends at
            // the right inset.
            i18n::set_locale("ar");
            let mut win = AppWindow::new("t", 420, 200);
            let mut body = win.body();
            let text = "Copies the selected files to the other pane\nand keeps them";
            let c = Button::new("Copy").bordered().tooltip(text);
            body.fit(&*c);
            win.end();
            lay_out(&mut win, shots);
            let rtl = layout::is_rtl(&*c);
            check(tooltip::show(&*c), "show rtl")?;
            let s = tooltip::shown().ok_or("rtl shown")?;
            let p = shots.join("tooltip-rtl.png");
            win.snapshot(&p).map_err(|e| e.to_string())?;
            i18n::set_locale("en");
            check(rtl, "the window is right to left")?;
            let img = png_rgb(&p)?;
            let (want_fill, _) = expected();
            let (x, y, w, h) = s.rect;
            let inset = theme::with(|t| t.spacing.control_padding_x + t.tooltip.border_width);
            let pad_y = theme::with(|t| t.spacing.control_padding_y + t.tooltip.border_width);
            // Two lines of different length share their right edge.
            let line_h = (h - 2 * pad_y) / 2;
            let edges = |top: i32| {
                let (mut lo, mut hi) = (i32::MAX, i32::MIN);
                for yy in top..top + line_h {
                    for xx in x + inset - 2..x + w - inset + 2 {
                        if img.at(xx, yy) != want_fill {
                            lo = lo.min(xx);
                            hi = hi.max(xx);
                        }
                    }
                }
                (lo, hi)
            };
            let (a, b) = (edges(y + pad_y), edges(y + pad_y + line_h));
            check(
                s.rect.3 > 2 * line_h && (a.1 - b.1).abs() <= 3 && (a.0 - b.0).abs() > 40,
                &format!("right-aligned lines: {a:?} {b:?} in {:?}", s.rect),
            )
        },
    );
    #[cfg(feature = "a11y")]
    run(
        "tooltip: stays the accessible description; the surface is not exposed",
        &|shots| {
            let mut win = AppWindow::new("t", 420, 200);
            let mut body = win.body();
            let b = Button::new("Save").bordered().tooltip("Save the file");
            body.fit(&*b);
            win.end();
            lay_out(&mut win, shots);
            check(tooltip::show(&*b), "show")?;
            let tree = fltk_omarchy::a11y::snapshot(&*win).unwrap_or_default();
            tooltip::hide();
            check(
                tree.contains("Save the file"),
                &format!("described: {tree}"),
            )?;
            check(
                tree.matches("Save the file").count() == 1,
                &format!("one node says it: {tree}"),
            )
        },
    );
}

fn tip_parent_is<W: WidgetExt>(win: &W) -> bool {
    let Some(s) = tooltip::shown() else {
        return false;
    };
    // The layer is the last child of the window and has the tooltip rect.
    let Some(g) = win.as_group() else {
        return false;
    };
    (0..g.children())
        .filter_map(|i| g.child(i))
        .any(|c| (c.x(), c.y(), c.w(), c.h()) == s.rect && !c.active())
}
