//! Drawing helpers shared by the components: Omarchy's BorderSurface
//! (fill + inner border, `Style.cornerRadius` corners), text with the font
//! tokens, and Nerd Font glyphs. Only call these inside a draw callback.

pub use crate::anim::Look;
use crate::theme::{Rgb, StateStyle, Theme};
use fltk::draw;
use fltk::enums::{Align, Font};

/// Interaction state of a control, in Omarchy's priority order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Interaction {
    pub hover: bool,
    pub pressed: bool,
    pub focus: bool,
    pub selected: bool,
    pub disabled: bool,
}

/// Filled rectangle; rounded when `radius > 0`.
pub fn fill(x: i32, y: i32, w: i32, h: i32, radius: i32, color: Rgb) {
    if w <= 0 || h <= 0 {
        return;
    }
    draw::set_draw_color(color.fl());
    let r = radius.min(w / 2).min(h / 2);
    if r > 0 { draw::draw_rounded_rectf(x, y, w, h, r) } else { draw::draw_rectf(x, y, w, h) }
}

/// Inner stroke of `width` pixels (Qt Rectangle border semantics).
pub fn stroke(x: i32, y: i32, w: i32, h: i32, radius: i32, width: i32, color: Rgb) {
    draw::set_draw_color(color.fl());
    for i in 0..width.max(0) {
        let (xx, yy, ww, hh) = (x + i, y + i, w - 2 * i, h - 2 * i);
        if ww <= 0 || hh <= 0 {
            break;
        }
        let r = (radius - i).max(0).min(ww / 2).min(hh / 2);
        if r > 0 { draw::draw_rounded_rect(xx, yy, ww, hh, r) } else { draw::draw_rect(xx, yy, ww, hh) }
    }
}

/// BorderSurface: fill then inner border.
pub fn surface(x: i32, y: i32, w: i32, h: i32, radius: i32, fill_color: Option<Rgb>, border: Option<(Rgb, i32)>) {
    if let Some(c) = fill_color {
        fill(x, y, w, h, radius, c);
    }
    if let Some((c, bw)) = border {
        if bw > 0 {
            stroke(x, y, w, h, radius, bw, c);
        }
    }
}

/// Paints a control background with the `[controls]` state tokens:
/// fill priority pressed > focus > hover > selected > `base`, border
/// priority focus > hover > selected (if width > 0) > normal (if `bordered`).
pub fn control(t: &Theme, x: i32, y: i32, w: i32, h: i32, st: Interaction, under: Rgb, base: Option<Rgb>, bordered: bool) {
    draw_look(x, y, w, h, t.radius, control_look(t, st, under, base, bordered), under);
}

/// The background [`control`] would paint, as a value that can be animated.
pub fn control_look(t: &Theme, st: Interaction, under: Rgb, base: Option<Rgb>, bordered: bool) -> Look {
    let (fill, border) = control_parts(t, st, under, base, bordered);
    let f = fill.unwrap_or(under);
    match border {
        Some((c, w)) => Look { fill: f, border: c, border_width: w },
        None => Look { fill: f, border: f, border_width: 0 },
    }
}

/// Draws a [`Look`] (no fill is drawn when it equals `under`).
pub fn draw_look(x: i32, y: i32, w: i32, h: i32, radius: i32, look: Look, under: Rgb) {
    let fill = (look.fill != under).then_some(look.fill);
    surface(x, y, w, h, radius, fill, (look.border_width > 0).then_some((look.border, look.border_width)));
}

fn control_parts(t: &Theme, st: Interaction, under: Rgb, base: Option<Rgb>, bordered: bool) -> (Option<Rgb>, Option<(Rgb, i32)>) {
    let c = &t.controls;
    let fill_color = if st.pressed {
        Some(c.hover.color.over(under, c.pressed_fill_alpha))
    } else if st.focus {
        Some(t.fill(&c.focus, under))
    } else if st.hover {
        Some(t.fill(&c.hover, under))
    } else if st.selected {
        Some(t.fill(&c.selected, under))
    } else {
        base
    };
    let border = |s: &StateStyle| Some((t.border(s, fill_color.unwrap_or(under)), s.border_width));
    let border = if st.focus {
        border(&c.focus)
    } else if st.hover {
        border(&c.hover)
    } else if st.selected && c.selected.border_width > 0 {
        border(&c.selected)
    } else if bordered {
        border(&c.normal)
    } else {
        None
    };
    (fill_color, border)
}

pub fn font(bold: bool) -> Font {
    if bold { Font::HelveticaBold } else { Font::Helvetica }
}

/// Text in a box with alignment; clipped and single-line unless `Align::Wrap`.
pub fn text(s: &str, x: i32, y: i32, w: i32, h: i32, size: i32, bold: bool, color: Rgb, align: Align) {
    draw::set_font(font(bold), size);
    draw::set_draw_color(color.fl());
    draw::draw_text2(&escape(s), x, y, w, h, align | Align::Inside | Align::Clip);
}

/// Like [`text`] but cuts with "…" when it does not fit (Text.ElideRight).
pub fn text_elided(s: &str, x: i32, y: i32, w: i32, h: i32, size: i32, bold: bool, color: Rgb, align: Align) {
    draw::set_font(font(bold), size);
    let fitted = elide(s, w);
    text(&fitted, x, y, w, h, size, bold, color, align);
}

/// Width of `s` with the current font settings.
pub fn measure(s: &str, size: i32, bold: bool) -> i32 {
    draw::set_font(font(bold), size);
    draw::width(s).ceil() as i32
}

/// Elides with "…" to fit `max_w` pixels (uses the current font).
pub fn elide(s: &str, max_w: i32) -> String {
    if (draw::width(s).ceil() as i32) <= max_w {
        return s.to_string();
    }
    let mut out: String = s.to_string();
    while !out.is_empty() {
        out.pop();
        let candidate = format!("{}…", out.trim_end());
        if (draw::width(&candidate).ceil() as i32) <= max_w {
            return candidate;
        }
    }
    String::new()
}

/// A glyph centered in a box (Nerd Font icons).
pub fn glyph(g: &str, x: i32, y: i32, w: i32, h: i32, size: i32, color: Rgb) {
    text(g, x, y, w, h, size, false, color, Align::Center);
}

/// FLTK treats '@' as a symbol prefix in labels; draw it literally.
pub fn escape(s: &str) -> String {
    s.replace('@', "@@")
}

/// Horizontal hairline (PanelSeparator) `rgba(fg, strength)`.
pub fn hairline(x: i32, y: i32, w: i32, fg: Rgb, under: Rgb, strength: f64) {
    fill(x, y, w, 1, 0, fg.over(under, strength));
}

/// Alpha-blended rectangle over existing pixels (scrims, translucent cards).
pub fn alpha_fill(x: i32, y: i32, w: i32, h: i32, color: Rgb, alpha: f64) {
    use fltk::prelude::ImageExt;
    if w <= 0 || h <= 0 {
        return;
    }
    let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    if let Ok(mut img) = fltk::image::RgbImage::new(&[color.r(), color.g(), color.b(), a], 1, 1, fltk::enums::ColorDepth::Rgba8) {
        img.scale(w, h, false, true);
        img.draw(x, y, w, h);
    }
}
