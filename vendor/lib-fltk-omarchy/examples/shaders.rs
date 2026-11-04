//! Shapes, paints and CPU shaders (software rendering, colors from the theme).
//!   cargo run --example shaders
//!   cargo run --example shaders -- --snapshot docs/components   (writes gfx-*.png)
use fltk::prelude::*;
use fltk_omarchy::gfx::{self, BlendMode, Cap, Color, Hit, Join, Paint, Path, Rate, Stroke, View};
use fltk_omarchy::layout::{self, FlexExt};
use fltk_omarchy::widgets::*;
use fltk_omarchy::{App, AppWindow};
use std::f32::consts::PI;

fn star(cx: f32, cy: f32, r: f32, inner: f32, points: usize) -> Path {
    let pts: Vec<gfx::Point> = (0..points * 2)
        .map(|i| {
            let a = i as f32 * PI / points as f32 - PI / 2.0;
            let rr = if i % 2 == 0 { r } else { inner };
            (cx + rr * a.cos(), cy + rr * a.sin())
        })
        .collect();
    Path::polygon(&pts)
}

/// Shapes: per-corner radii, squircle, star, Bézier strokes with joins and caps.
fn shapes() -> View {
    View::new(560, 150).paint(|c, u| {
        let fg = u.foreground;
        c.fill(&Path::rounded_rect(10.0, 20.0, 110.0, 110.0, [28.0, 4.0, 28.0, 4.0]), u.accent);
        c.fill(&Path::squircle(140.0, 20.0, 110.0, 110.0, 4.5), u.accent.mix(u.background, 0.35));
        c.fill(&star(325.0, 75.0, 58.0, 26.0, 5), u.accent.mix(u.foreground, 0.3));
        let wave = Path::new().move_to(400.0, 110.0).cubic_to(430.0, 10.0, 470.0, 140.0, 545.0, 40.0);
        c.stroke(&wave, &Stroke::new(10.0).cap(Cap::Round), fg.with_alpha(0.35));
        c.stroke(&wave, &Stroke::new(3.0).cap(Cap::Round), u.accent);
        let zig = Path::new().move_to(400.0, 135.0).line_to(430.0, 115.0).line_to(460.0, 135.0).line_to(490.0, 115.0);
        c.stroke(&zig, &Stroke::new(6.0).join(Join::Round).cap(Cap::Round), fg);
        let zig2 = zig.translate(60.0, 0.0);
        c.stroke(&zig2, &Stroke::new(6.0).join(Join::Miter(4.0)), fg.with_alpha(0.6));
    })
}

/// Paints: linear, radial and conic gradients built from theme colors.
fn paints() -> View {
    View::new(560, 130).paint(|c, u| {
        let (a, b, f) = (u.accent, u.background, u.foreground);
        c.fill(&Path::rounded_rect(10.0, 10.0, 170.0, 110.0, [12.0; 4]), Paint::linear((10.0, 0.0), (180.0, 0.0), &[(0.0, a), (1.0, b.mix(f, 0.15))]));
        c.fill(&Path::circle(285.0, 65.0, 55.0), Paint::radial((270.0, 50.0), 70.0, &[(0.0, a.mix(f, 0.5)), (0.6, a), (1.0, b)]));
        c.fill(&Path::circle(450.0, 65.0, 55.0), Paint::conic((450.0, 65.0), -PI / 2.0, &[(0.0, a), (0.5, f.mix(b, 0.5)), (1.0, a)]));
        c.fill(&Path::circle(450.0, 65.0, 30.0), b);
    })
}

/// Effects: soft shadow, layer opacity, blend modes, clip by shape.
fn effects() -> View {
    View::new(560, 150).paint(|c, u| {
        let card = Path::rounded_rect(20.0, 20.0, 140.0, 100.0, [14.0; 4]);
        c.shadow(&card, 0.0, 8.0, 12, Color::rgba(0.0, 0.0, 0.0, 0.6));
        c.fill(&card, u.background.mix(u.foreground, 0.12));
        // Group opacity: the overlap is not darker.
        c.layer(0.6, BlendMode::Normal, |c| {
            c.fill(&Path::circle(225.0, 70.0, 40.0), u.accent);
            c.fill(&Path::circle(265.0, 70.0, 40.0), u.accent);
        });
        // Additive glow (Screen/Multiply/Darken/Lighten also exist).
        c.blend = BlendMode::Add;
        c.fill(&Path::circle(370.0, 60.0, 34.0), u.accent.with_alpha(0.7));
        c.fill(&Path::circle(395.0, 85.0, 34.0), u.foreground.with_alpha(0.5));
        c.blend = BlendMode::Normal;
        // Clip by shape: stripes only inside the squircle.
        c.clipped(&Path::squircle(450.0, 20.0, 100.0, 110.0, 4.0), |c| {
            for i in 0..14 {
                let x = 440.0 + i as f32 * 9.0;
                c.fill(&Path::polygon(&[(x, 10.0), (x + 4.0, 10.0), (x - 26.0, 140.0), (x - 30.0, 140.0)]), u.accent);
            }
        });
    })
}

fn main() {
    let shots = std::env::args().skip_while(|a| a != "--snapshot").nth(1);
    let app = App::new("dev.omarchy.fltk.shaders");
    let mut win = AppWindow::new("Shapes and shaders", 600, 900);
    let mut body = win.body();
    body.fit(&*Label::section("Shapes"));
    let s = shapes();
    body.fit(&*s);
    body.fit(&*Label::section("Paints"));
    let p = paints();
    body.fit(&*p);
    body.fit(&*Label::section("Effects"));
    let e = effects();
    body.fit(&*e);
    body.fit(&*Label::section("Shaders (animated; GPU or CPU)"));
    let mut row = layout::row();
    let noise = View::new(130, 110).shader(gfx::shaders::noise()).animate(Rate::Display);
    let grad = View::new(130, 110).shader(gfx::shaders::gradient()).animate(Rate::Display);
    let glow = View::new(130, 110).shader(gfx::shaders::accent_glow()).animate(Rate::Display);
    // Background blur: moving blobs, the panel area blurred, a card on top.
    let frosted = View::new(130, 110)
        .shader(gfx::shaders::blobs())
        .post(|c, u| {
            c.blur(15, 25, 100, 60, 10);
            c.fill(&Path::rounded_rect(15.0, 25.0, 100.0, 60.0, [10.0; 4]), u.background.with_alpha(0.25));
            c.stroke(&Path::rounded_rect(15.5, 25.5, 99.0, 59.0, [10.0; 4]), &Stroke::new(1.0), u.foreground.with_alpha(0.35));
        })
        .animate(Rate::Display);
    for v in [&noise, &grad, &glow, &frosted] {
        row.fit(&**v);
    }
    row.close_fitted();
    body.fit(&*Label::section("Shaped button (click or Tab + Enter)"));
    let clicks = fltk_omarchy::State::new(0u32);
    let c2 = clicks.clone();
    let shape = star(60.0, 45.0, 42.0, 20.0, 6);
    let hit = shape.clone();
    let mut r = layout::row();
    let btn = View::new(120, 90)
        .paint(move |c, u| c.fill(&shape, Paint::radial((60.0, 40.0), 45.0, &[(0.0, u.accent.mix(u.foreground, 0.4)), (1.0, u.accent)])))
        .hit_shape(Hit::Fill(hit))
        .on_click(move || c2.update(|n| *n += 1));
    r.fit(&*btn);
    r.fit(&*Label::new("").bind(&clicks.map(|n| format!("{n} clicks inside the star"))));
    r.close_fitted();
    win.end();

    if let Some(dir) = shots {
        fltk_omarchy::anim::set_enabled(false);
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        for v in [&noise, &grad, &glow, &frosted] {
            v.set_time(Some(1.3));
        }
        win.snapshot(dir.join("gfx-gallery.png")).unwrap();
        // One picture per section (offscreen windows of the same views).
        for (name, view) in [("gfx-shapes", shapes()), ("gfx-paints", paints()), ("gfx-effects", effects())] {
            let mut w = fltk::window::DoubleWindow::new(0, 0, view.w(), view.h(), None);
            w.add(&*view);
            w.end();
            fltk_omarchy::window::snapshot(&mut w, &dir.join(format!("{name}.png"))).unwrap();
        }
        println!("written to {}", dir.display());
        return;
    }
    win.show();
    app.run();
}
