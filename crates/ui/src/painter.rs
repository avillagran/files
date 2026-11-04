use files_core::theme::Color;
use fltk::{
    draw,
    enums::{Align, Font},
    image::RgbImage,
    prelude::*,
};
#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}
impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}
pub struct Image<'a> {
    pub rgb: &'a [u8],
    pub width: i32,
    pub height: i32,
}
pub struct EffectId(pub &'static str);
pub struct Uniforms {
    pub time: f32,
    pub progress: f32,
    pub color: Color,
}
pub struct TextureId(pub u64);
pub trait Painter {
    fn rect(&mut self, r: Rect, c: Color);
    fn rounded_rect(&mut self, r: Rect, radius: i32, c: Color);
    fn text(&mut self, r: Rect, text: &str, size: i32, c: Color, mono: bool, right: bool);
    /// Literal content: callers expand tabs/controls and preserve their own byte map.
    fn literal_text(&mut self, r: Rect, text: &str, size: i32, c: Color) {
        self.text(r, text, size, c, true, false);
    }
    /// Backends should return their shaped advance for pointer/text alignment.
    /// Conservative fallback is used by headless test painters only.
    fn text_width(&mut self, text: &str, size: i32, _mono: bool) -> f32 {
        text.chars().count() as f32 * size as f32 * 0.6
    }
    fn styled_text(
        &mut self,
        r: Rect,
        text: &str,
        size: i32,
        c: Color,
        _bold: bool,
        _italic: bool,
    ) {
        self.literal_text(r, text, size, c);
    }
    fn styled_width(&mut self, text: &str, size: i32, _bold: bool, _italic: bool) -> f32 {
        self.text_width(text, size, true)
    }
    fn image(&mut self, r: Rect, image: &Image<'_>);
    fn clip(&mut self, r: Rect);
    fn unclip(&mut self);
    fn effect(&mut self, _region: Rect, _id: EffectId, _u: &Uniforms) -> bool {
        false
    }
    fn capture(&mut self, _region: Rect) -> Option<TextureId> {
        None
    }
}
pub struct FltkPainter;
fn color(c: Color) -> fltk::enums::Color {
    fltk::enums::Color::from_rgb(c.0, c.1, c.2)
}
/// Files paints every surface as a toolkit glass surface: opaque as before,
/// or with Omarchy's glass opacity under Glassy windows (text and images
/// drawn over it stay opaque).
fn glass(c: Color) -> fltk_omarchy::theme::Rgb {
    fltk_omarchy::theme::Rgb(u32::from(c.0) << 16 | u32::from(c.1) << 8 | u32::from(c.2))
}
impl Painter for FltkPainter {
    fn rect(&mut self, r: Rect, c: Color) {
        fltk_omarchy::glass::surface(glass(c), || draw::draw_rectf(r.x, r.y, r.w, r.h));
    }
    fn rounded_rect(&mut self, r: Rect, radius: i32, c: Color) {
        fltk_omarchy::glass::surface(glass(c), || {
            if radius == 0 {
                draw::draw_rectf(r.x, r.y, r.w, r.h);
            } else {
                draw::draw_rounded_rectf(r.x, r.y, r.w, r.h, radius);
            }
        });
    }
    fn text(&mut self, r: Rect, text: &str, size: i32, c: Color, mono: bool, right: bool) {
        draw::set_draw_color(color(c));
        draw::set_font(if mono { Font::Courier } else { Font::Helvetica }, size);
        draw::push_clip(r.x, r.y, r.w.max(0), r.h.max(0));
        let a = if right { Align::Right } else { Align::Left };
        draw::draw_text2(text, r.x, r.y, r.w, r.h, a | Align::Inside | Align::Clip);
        draw::pop_clip();
    }
    fn literal_text(&mut self, r: Rect, text: &str, size: i32, c: Color) {
        draw::set_font(Font::Courier, size);
        draw::set_draw_color(color(c));
        draw::push_clip(r.x, r.y, r.w.max(0), r.h.max(0));
        let baseline = r.y + (r.h - draw::height()) / 2 + draw::height() - draw::descent();
        draw::draw_text(text, r.x, baseline);
        draw::pop_clip();
    }
    fn text_width(&mut self, text: &str, size: i32, mono: bool) -> f32 {
        draw::set_font(if mono { Font::Courier } else { Font::Helvetica }, size);
        draw::width(text) as f32
    }
    fn styled_text(&mut self, r: Rect, text: &str, size: i32, c: Color, bold: bool, italic: bool) {
        draw::set_font(styled_font(bold, italic), size);
        draw::set_draw_color(color(c));
        draw::push_clip(r.x, r.y, r.w.max(0), r.h.max(0));
        let baseline = r.y + (r.h - draw::height()) / 2 + draw::height() - draw::descent();
        draw::draw_text(text, r.x, baseline);
        draw::pop_clip();
    }
    fn styled_width(&mut self, text: &str, size: i32, bold: bool, italic: bool) -> f32 {
        draw::set_font(styled_font(bold, italic), size);
        draw::width(text) as f32
    }
    fn image(&mut self, r: Rect, img: &Image<'_>) {
        if let Ok(mut i) = RgbImage::new(
            img.rgb,
            img.width,
            img.height,
            fltk::enums::ColorDepth::Rgb8,
        ) {
            i.draw(r.x, r.y, r.w, r.h);
        }
    }
    fn clip(&mut self, r: Rect) {
        draw::push_clip(r.x, r.y, r.w.max(0), r.h.max(0));
    }
    fn unclip(&mut self) {
        draw::pop_clip();
    }
}

fn styled_font(bold: bool, italic: bool) -> Font {
    match (bold, italic) {
        (true, true) => Font::CourierBoldItalic,
        (true, false) => Font::CourierBold,
        (false, true) => Font::CourierItalic,
        _ => Font::Courier,
    }
}
