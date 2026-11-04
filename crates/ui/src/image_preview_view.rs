//! Viewport-sized software sampling: zoom never allocates an enlarged full image.
use crate::{
    image_preview_job::Bitmap,
    painter::{Image, Painter, Rect},
};
use files_core::theme::Palette;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub quarter_turns: u8,
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            zoom: 1.,
            pan_x: 0.,
            pan_y: 0.,
            quarter_turns: 0,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct SampleKey {
    width: i32,
    height: i32,
    zoom: u32,
    pan: [u32; 2],
    background: [u8; 3],
    radius: i32,
    quarter_turns: u8,
}
#[derive(Default)]
pub struct Viewport {
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub body: Rect,
    pub drag: Option<(i32, i32, f32, f32)>,
    quarter_turns: u8,
    buffer: Vec<u8>,
    key: Option<SampleKey>,
}
impl Viewport {
    /// Invalidate frame pixels without discarding the user's current framing.
    pub fn refresh_pixels(&mut self) {
        self.key = None;
    }
    pub fn transform(&self) -> Transform {
        Transform {
            zoom: self.zoom.max(1.),
            pan_x: self.pan_x,
            pan_y: self.pan_y,
            quarter_turns: self.quarter_turns,
        }
    }
    pub fn restore_transform(&mut self, transform: Transform) {
        self.zoom = if transform.zoom.is_finite() {
            transform.zoom.clamp(1., 8.)
        } else {
            1.
        };
        self.pan_x = if transform.pan_x.is_finite() {
            transform.pan_x.max(0.)
        } else {
            0.
        };
        self.pan_y = if transform.pan_y.is_finite() {
            transform.pan_y.max(0.)
        } else {
            0.
        };
        self.quarter_turns = transform.quarter_turns % 4;
        self.key = None;
        self.drag = None;
    }
    pub fn quarter_turns(&self) -> u8 {
        self.quarter_turns
    }
    pub fn rotate_by(&mut self, delta: i32) {
        self.quarter_turns =
            (i32::from(self.quarter_turns) + delta.rem_euclid(4)).rem_euclid(4) as u8;
        self.pan_x = 0.;
        self.pan_y = 0.;
        self.drag = None;
        self.key = None;
    }
    pub fn zoom_by(&mut self, factor: f32) {
        self.zoom = (self.zoom.max(1.0) * factor).clamp(1.0, 8.0);
    }
    /// A different bitmap may have identical dimensions. Invalidate its content
    /// key while retaining the single bounded resampling allocation.
    pub fn reset_content(&mut self) {
        self.key = None;
        self.reset();
        self.drag = None;
    }
    pub fn buffer_capacity(&self) -> usize {
        self.buffer.capacity()
    }
    pub fn reset(&mut self) {
        self.zoom = 1.0;
        self.pan_x = 0.0;
        self.pan_y = 0.0;
        self.quarter_turns = 0;
        self.drag = None;
    }
    pub fn paint(&mut self, p: &mut dyn Painter, b: &Bitmap, body: Rect, c: &Palette) {
        self.paint_inner(p, b, body, c, 8);
    }
    pub fn paint_thumbnail(&mut self, p: &mut dyn Painter, b: &Bitmap, body: Rect, c: &Palette) {
        self.paint_inner(p, b, body, c, 8);
    }
    fn paint_inner(
        &mut self,
        p: &mut dyn Painter,
        b: &Bitmap,
        body: Rect,
        c: &Palette,
        radius: i32,
    ) {
        self.body = body;
        p.rect(body, c.background);
        let w = body.w.clamp(1, 1160);
        let h = body.h.clamp(1, 800);
        // Sample in device pixels (2× on a HiDPI output) so frames decoded at
        // the preview's real resolution reach the screen pixel for pixel.
        // Pan and pointer math stay in logical units.
        let s = crate::painter::device_scale();
        let (dev_w, dev_h) = (
            ((w as f32 * s).round() as i32).max(1),
            ((h as f32 * s).round() as i32).max(1),
        );
        let zoom = self.zoom.max(1.0).clamp(1.0, 8.0);
        let (source_w, source_h) = if self.quarter_turns % 2 == 1 {
            (b.height, b.width)
        } else {
            (b.width, b.height)
        };
        // Never larger than one source pixel per logical pixel at zoom 1, as
        // before; on a 2× output that is two device pixels.
        let scale = (dev_w as f32 / source_w as f32)
            .min(dev_h as f32 / source_h as f32)
            .min(s)
            * zoom;
        let dw = source_w as f32 * scale;
        let dh = source_h as f32 * scale;
        self.pan_x = self.pan_x.clamp(0.0, ((dw - dev_w as f32) / s).max(0.0));
        self.pan_y = self.pan_y.clamp(0.0, ((dh - dev_h as f32) / s).max(0.0));
        let key = SampleKey {
            width: dev_w,
            height: dev_h,
            zoom: zoom.to_bits(),
            pan: [self.pan_x.to_bits(), self.pan_y.to_bits()],
            background: [c.background.0, c.background.1, c.background.2],
            radius,
            quarter_turns: self.quarter_turns,
        };
        if self.key != Some(key) {
            let needed = dev_w as usize * dev_h as usize * 3;
            if self.buffer.capacity() < needed {
                self.buffer.reserve_exact(needed - self.buffer.len());
            }
            self.buffer.resize(needed, 0);
            let left = (dev_w as f32 - dw).max(0.0) / 2.0 - self.pan_x * s;
            let top = (dev_h as f32 - dh).max(0.0) / 2.0 - self.pan_y * s;
            let background = [c.background.0, c.background.1, c.background.2];
            let radius = (radius as f32 * s).min(dw / 2.).min(dh / 2.);
            // Source column per device column, computed once per frame.
            let columns: Vec<i32> = (0..dev_w)
                .map(|x| {
                    let sx = ((x as f32 - left) / scale).floor() as i32;
                    if sx >= 0 && sx < source_w as i32 {
                        sx
                    } else {
                        -1
                    }
                })
                .collect();
            let row_bytes = b.width as usize * 3;
            for y in 0..dev_h {
                let sy = ((y as f32 - top) / scale).floor() as i32;
                let row = y as usize * dev_w as usize * 3;
                let inside_y = sy >= 0 && sy < source_h as i32;
                let fy = y as f32 - top;
                let corner_row = radius > 0. && (fy < radius || fy > dh - radius - 1.);
                for (x, &sx) in columns.iter().enumerate() {
                    let i = row + x * 3;
                    let rgb = if inside_y && sx >= 0 {
                        let (sx, sy) = (sx as u32, sy as u32);
                        let (raw_x, raw_y) = match self.quarter_turns {
                            1 => (sy, b.height - 1 - sx),
                            2 => (b.width - 1 - sx, b.height - 1 - sy),
                            3 => (b.width - 1 - sy, sx),
                            _ => (sx, sy),
                        };
                        let j = raw_y as usize * row_bytes + raw_x as usize * 3;
                        [b.rgb[j], b.rgb[j + 1], b.rgb[j + 2]]
                    } else {
                        background
                    };
                    // Round the displayed image itself, including letterboxed
                    // thumbnails, rather than the surrounding sampling viewport.
                    let fx = x as f32 - left;
                    if corner_row && (fx < radius || fx > dw - radius - 1.) {
                        let dx = (radius - 0.5 - fx).max(fx - (dw - radius - 0.5)).max(0.);
                        let dy = (radius - 0.5 - fy).max(fy - (dh - radius - 0.5)).max(0.);
                        let alpha = if dx > 0. && dy > 0. {
                            (radius + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0., 1.)
                        } else {
                            1.
                        };
                        for channel in 0..3 {
                            self.buffer[i + channel] = (rgb[channel] as f32 * alpha
                                + background[channel] as f32 * (1. - alpha))
                                .round()
                                as u8;
                        }
                    } else {
                        self.buffer[i..i + 3].copy_from_slice(&rgb);
                    }
                }
            }
            self.key = Some(key);
        }
        p.clip(body);
        p.image_scaled(
            Rect::new(body.x, body.y, w, h),
            &Image {
                rgb: &self.buffer,
                width: dev_w,
                height: dev_h,
            },
        );
        p.unclip();
    }
    /// Device-pixel box this viewport shows images in (aspect-fit target for
    /// decoders), for a body of `w`×`h` logical pixels.
    pub fn device_box(w: i32, h: i32) -> (u32, u32) {
        let s = crate::painter::device_scale();
        (
            ((w.clamp(1, 1160) as f32 * s).round() as u32).max(1),
            ((h.clamp(1, 800) as f32 * s).round() as u32).max(1),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use files_core::theme::Color;
    #[derive(Default)]
    struct Recorder {
        size: usize,
        corner: [u8; 3],
    }
    impl Painter for Recorder {
        fn rect(&mut self, _: Rect, _: Color) {}
        fn rounded_rect(&mut self, _: Rect, _: i32, _: Color) {}
        fn text(&mut self, _: Rect, _: &str, _: i32, _: Color, _: bool, _: bool) {}
        fn clip(&mut self, _: Rect) {}
        fn unclip(&mut self) {}
        fn image(&mut self, _: Rect, i: &Image<'_>) {
            self.size = i.rgb.len();
            self.corner.copy_from_slice(&i.rgb[..3]);
        }
    }
    #[test]
    fn quarter_turns_sample_asymmetric_matrix_without_rotated_source_allocation() {
        let b = Bitmap {
            source: None,
            width: 3,
            height: 2,
            source_width: 3,
            source_height: 2,
            format: "png".into(),
            rgb: (1..=6).flat_map(|v| [v, v, v]).collect(),
        };
        let mut v = Viewport::default();
        let c = Palette::default();
        for (turn, expected) in [
            (0, vec![1, 2, 3, 4, 5, 6]),
            (1, vec![4, 1, 5, 2, 6, 3]),
            (2, vec![6, 5, 4, 3, 2, 1]),
            (3, vec![3, 6, 2, 5, 1, 4]),
        ] {
            v.reset();
            v.rotate_by(turn);
            let (w, h) = if turn % 2 == 1 { (2, 3) } else { (3, 2) };
            v.paint_inner(&mut Recorder::default(), &b, Rect::new(0, 0, w, h), &c, 0);
            assert_eq!(
                v.buffer.iter().step_by(3).copied().collect::<Vec<_>>(),
                expected
            );
            assert_eq!(v.buffer.len(), 18);
        }
        v.rotate_by(i32::MAX);
        v.rotate_by(i32::MIN);
        assert!(v.quarter_turns() < 4);
        let t = v.transform();
        v.refresh_pixels();
        assert_eq!(v.transform(), t);
        let mut other = Viewport::default();
        other.restore_transform(t);
        assert_eq!(other.transform(), t);
        assert_eq!(other.buffer_capacity(), 0);
        v.reset_content();
        assert_eq!(v.quarter_turns(), 0);
    }
    #[test]
    fn frame_refresh_preserves_transform_and_reuses_pixels_without_stale_content() {
        let mut v = Viewport::default();
        let mut b = Bitmap {
            source: None,
            width: 100,
            height: 100,
            source_width: 100,
            source_height: 100,
            format: "frame".into(),
            rgb: vec![255; 30000],
        };
        let mut p = Recorder::default();
        let c = Palette::default();
        v.zoom_by(2.);
        v.pan_x = 12.;
        v.pan_y = 14.;
        let body = Rect::new(0, 0, 80, 80);
        v.paint(&mut p, &b, body, &c);
        let cap = v.buffer_capacity();
        let transform = v.transform();
        b.rgb.fill(23);
        v.refresh_pixels();
        v.paint(&mut p, &b, body, &c);
        assert_eq!(v.transform(), transform);
        assert_eq!(v.buffer_capacity(), cap);
        assert_eq!(v.buffer[(40 * 80 + 40) * 3], 23);
        v.reset_content();
        assert_eq!(v.transform(), Transform::default());
        let mut restored = Viewport::default();
        restored.restore_transform(transform);
        assert_eq!(restored.transform(), transform);
        assert_eq!(
            restored.buffer_capacity(),
            0,
            "seeks retain numbers, never raster history"
        );
    }
    #[test]
    fn letterboxed_previews_and_thumbnails_round_actual_image_corners() {
        let mut v = Viewport::default();
        let b = Bitmap {
            source: None,
            width: 160,
            height: 100,
            source_width: 160,
            source_height: 100,
            format: "png".into(),
            rgb: vec![255; 160 * 100 * 3],
        };
        let c = Palette {
            background: Color(12, 34, 56),
            ..Palette::default()
        };
        for thumbnail in [false, true] {
            if thumbnail {
                v.paint_thumbnail(&mut Recorder::default(), &b, Rect::new(0, 0, 160, 160), &c);
            } else {
                v.paint(&mut Recorder::default(), &b, Rect::new(0, 0, 160, 160), &c);
            }
            assert_eq!(&v.buffer[30 * 160 * 3..30 * 160 * 3 + 3], &[12, 34, 56]);
            assert_eq!(
                &v.buffer[(38 * 160 + 8) * 3..(38 * 160 + 8) * 3 + 3],
                &[255, 255, 255]
            );
        }
    }
    #[test]
    fn thumbnails_are_rounded_and_reuse_one_bounded_sampling_buffer() {
        let mut v = Viewport::default();
        let b = Bitmap {
            source: None,
            width: 160,
            height: 160,
            source_width: 160,
            source_height: 160,
            format: "png".into(),
            rgb: vec![255; 160 * 160 * 3],
        };
        let c = Palette {
            background: Color(12, 34, 56),
            ..Palette::default()
        };
        let mut p = Recorder::default();
        v.paint_thumbnail(&mut p, &b, Rect::new(0, 0, 160, 160), &c);
        assert_eq!(p.corner, [12, 34, 56]);
        assert!(v.buffer_capacity() <= crate::thumbnail_cache::SCRATCH_BYTES);
        let cap = v.buffer_capacity();
        v.reset_content();
        v.paint_thumbnail(&mut p, &b, Rect::new(0, 0, 80, 70), &c);
        assert_eq!(v.buffer_capacity(), cap);
    }
    #[test]
    fn zoom_and_pan_allocate_only_visible_pixels_and_update_theme() {
        let b = Bitmap {
            source: None,
            width: 1024,
            height: 1024,
            source_width: 1024,
            source_height: 1024,
            format: "png".into(),
            rgb: vec![255; 1024 * 1024 * 3],
        };
        let mut v = Viewport::default();
        let mut p = Recorder::default();
        let mut c = Palette::default();
        v.zoom_by(1000.0);
        v.pan_x = 1e9;
        v.pan_y = 1e9;
        v.paint(&mut p, &b, Rect::new(0, 0, 5000, 5000), &c);
        assert_eq!(p.size, 1160 * 800 * 3);
        assert!(v.zoom <= 8.0);
        assert!(v.pan_x < 1e9 && v.pan_y < 1e9);
        let small = Bitmap {
            source: None,
            width: 1,
            height: 1,
            source_width: 1,
            source_height: 1,
            format: "png".into(),
            rgb: vec![255; 3],
        };
        let mut v = Viewport::default();
        v.paint(&mut p, &small, Rect::new(0, 0, 10, 10), &c);
        c.background = Color(1, 2, 3);
        v.paint(&mut p, &small, Rect::new(0, 0, 10, 10), &c);
        assert_eq!(p.corner, [1, 2, 3]);
        v.reset();
        assert_eq!(v.zoom, 1.0);
        assert_eq!(v.pan_x, 0.0);
    }
}
