//! Software canvas: an RGBA buffer you fill and stroke with paths, paints,
//! clips, layers, shadows and blur. Everything runs on the CPU.

use super::path::{Path, Point};
use super::raster::{stroke_polygons, Coverage, Stroke};
use crate::theme::Rgb;

/// Straight (non-premultiplied) color, components 0..1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };

    pub fn rgba(r: f32, g: f32, b: f32, a: f32) -> Color {
        Color { r, g, b, a }
    }

    /// `#rgb`, `#rrggbb` or `#rrggbbaa`.
    pub fn hex(s: &str) -> Option<Color> {
        let h = s.trim().strip_prefix('#')?;
        let alpha = if h.len() == 8 { u8::from_str_radix(&h[6..8], 16).ok()? as f32 / 255.0 } else { 1.0 };
        Rgb::parse(s).map(|c| Color::from(c).with_alpha(alpha))
    }

    pub fn with_alpha(self, a: f32) -> Color {
        Color { a, ..self }
    }

    /// Linear mix: `t = 0` is `self`, `t = 1` is `other`.
    pub fn mix(self, other: Color, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        let m = |a: f32, b: f32| a + (b - a) * t;
        Color { r: m(self.r, other.r), g: m(self.g, other.g), b: m(self.b, other.b), a: m(self.a, other.a) }
    }

    /// RGBA bytes (straight alpha).
    pub fn to_u8(self) -> [u8; 4] {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        [q(self.r), q(self.g), q(self.b), q(self.a)]
    }
}

impl From<Rgb> for Color {
    fn from(c: Rgb) -> Color {
        Color { r: c.r() as f32 / 255.0, g: c.g() as f32 / 255.0, b: c.b() as f32 / 255.0, a: 1.0 }
    }
}

/// What fills a shape. Build gradients from theme colors so they follow
/// `omarchy-theme-set`.
#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
    Solid(Color),
    /// Along the line `from → to`.
    Linear { from: Point, to: Point, stops: Vec<(f32, Color)> },
    /// From `center` (0) to `radius` (1).
    Radial { center: Point, radius: f32, stops: Vec<(f32, Color)> },
    /// Around `center`, starting at `angle` radians, clockwise.
    Conic { center: Point, angle: f32, stops: Vec<(f32, Color)> },
}

impl From<Color> for Paint {
    fn from(c: Color) -> Paint {
        Paint::Solid(c)
    }
}

impl From<Rgb> for Paint {
    fn from(c: Rgb) -> Paint {
        Paint::Solid(c.into())
    }
}

impl Paint {
    pub fn linear(from: Point, to: Point, stops: &[(f32, Color)]) -> Paint {
        Paint::Linear { from, to, stops: stops.to_vec() }
    }
    pub fn radial(center: Point, radius: f32, stops: &[(f32, Color)]) -> Paint {
        Paint::Radial { center, radius, stops: stops.to_vec() }
    }
    pub fn conic(center: Point, angle: f32, stops: &[(f32, Color)]) -> Paint {
        Paint::Conic { center, angle, stops: stops.to_vec() }
    }

    /// Color at pixel center `(x, y)`.
    pub fn at(&self, x: f32, y: f32) -> Color {
        match self {
            Paint::Solid(c) => *c,
            Paint::Linear { from, to, stops } => {
                let (dx, dy) = (to.0 - from.0, to.1 - from.1);
                let l2 = (dx * dx + dy * dy).max(1e-6);
                ramp(stops, ((x - from.0) * dx + (y - from.1) * dy) / l2)
            }
            Paint::Radial { center, radius, stops } => {
                let d = ((x - center.0).powi(2) + (y - center.1).powi(2)).sqrt();
                ramp(stops, d / radius.max(1e-6))
            }
            Paint::Conic { center, angle, stops } => {
                let a = (y - center.1).atan2(x - center.0) - angle;
                ramp(stops, a.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU)
            }
        }
    }
}

fn ramp(stops: &[(f32, Color)], t: f32) -> Color {
    match stops {
        [] => Color::TRANSPARENT,
        [(_, c)] => *c,
        _ => {
            let t = t.clamp(0.0, 1.0);
            if t <= stops[0].0 {
                return stops[0].1;
            }
            for w in stops.windows(2) {
                let ((t0, c0), (t1, c1)) = (w[0], w[1]);
                if t <= t1 {
                    return c0.mix(c1, if t1 > t0 { (t - t0) / (t1 - t0) } else { 1.0 });
                }
            }
            stops[stops.len() - 1].1
        }
    }
}

/// How new pixels combine with what is already there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlendMode {
    /// Source over (default).
    #[default]
    Normal,
    Multiply,
    Screen,
    /// Additive (glows).
    Add,
    Darken,
    Lighten,
}

/// Largest blur radius, in pixels (blur cost grows with the area, not the radius).
pub const MAX_BLUR: u32 = 32;

/// An RGBA pixel buffer with a small drawing state. Pixels are stored as
/// premultiplied RGBA8 (4 bytes per pixel); `pixel()` returns straight colors.
pub struct Canvas {
    w: usize,
    h: usize,
    px: Vec<u8>,
    /// Global opacity for the next draws (0..1).
    pub opacity: f32,
    /// Blend mode for the next draws.
    pub blend: BlendMode,
    clips: Vec<Vec<f32>>,
    layers: Vec<(Vec<u8>, f32, BlendMode)>,
    cov: Coverage,
    /// Canvas pixels per drawing unit (< 1 when a View renders below its
    /// widget size): coordinates passed in stay in widget pixels.
    unit: f32,
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Canvas {
        Canvas { w, h, px: vec![0; w * h * 4], opacity: 1.0, blend: BlendMode::Normal, clips: Vec::new(), layers: Vec::new(), cov: Coverage::new(), unit: 1.0 }
    }

    /// Drawing units are scaled by `unit` (View sets it when it renders at
    /// a lower resolution than the widget).
    pub fn set_unit(&mut self, unit: f32) {
        self.unit = unit.max(1e-3);
    }

    pub fn unit(&self) -> f32 {
        self.unit
    }

    fn sc(&self, p: &Path) -> Path {
        if self.unit == 1.0 { p.clone() } else { p.scale(self.unit, self.unit) }
    }

    pub fn width(&self) -> usize {
        self.w
    }

    pub fn height(&self) -> usize {
        self.h
    }

    /// Changes the size (contents cleared), keeping the allocation when it fits.
    pub fn resize(&mut self, w: usize, h: usize) {
        self.w = w;
        self.h = h;
        self.px.clear();
        self.px.resize(w * h * 4, 0);
        if self.px.capacity() > 2 * self.px.len() + 4096 {
            self.px.shrink_to_fit(); // much smaller than before: give memory back
        }
        self.clips.clear();
        self.layers.clear();
    }

    /// Bytes held by the pixel buffers (for budgets and measurements).
    pub fn memory(&self) -> usize {
        self.px.capacity() + self.clips.iter().map(|c| c.capacity() * 4).sum::<usize>()
    }

    /// Fills everything with `c` (ignores clip, opacity and blend).
    pub fn clear(&mut self, c: Color) {
        let p = [q8(c.r * c.a), q8(c.g * c.a), q8(c.b * c.a), q8(c.a)];
        for px in self.px.chunks_exact_mut(4) {
            px.copy_from_slice(&p);
        }
    }

    /// Straight color of a pixel.
    pub fn pixel(&self, x: usize, y: usize) -> Color {
        let i = (y * self.w + x) * 4;
        let a = self.px[i + 3] as f32 / 255.0;
        if a <= 0.0 {
            return Color::TRANSPARENT;
        }
        let f = |v: u8| (v as f32 / 255.0 / a).min(1.0);
        Color { r: f(self.px[i]), g: f(self.px[i + 1]), b: f(self.px[i + 2]), a }
    }

    /// Writes straight RGBA8 into `out` (resized to w × h × 4).
    pub fn to_rgba8(&self, out: &mut Vec<u8>) {
        out.resize(self.w * self.h * 4, 0);
        for (o, p) in out.chunks_exact_mut(4).zip(self.px.chunks_exact(4)) {
            let a = p[3] as u32;
            if a == 0 {
                o.copy_from_slice(&[0, 0, 0, 0]);
            } else {
                let q = |v: u8| ((v as u32 * 255 + a / 2) / a).min(255) as u8;
                o.copy_from_slice(&[q(p[0]), q(p[1]), q(p[2]), p[3]]);
            }
        }
    }

    /// Appends the raw premultiplied RGBA8 pixels (GPU overlay upload).
    #[cfg_attr(not(feature = "gpu"), allow(dead_code))]
    pub(crate) fn premul_bytes(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.px);
    }

    /// Opaque RGB8 over `bg` into `out` (resized to w × h × 3): the fast
    /// upload path for views that cover their whole area.
    pub fn to_rgb8_over(&self, bg: Color, out: &mut Vec<u8>) {
        out.resize(self.w * self.h * 3, 0);
        let b = [bg.r * 255.0, bg.g * 255.0, bg.b * 255.0];
        for (o, p) in out.chunks_exact_mut(3).zip(self.px.chunks_exact(4)) {
            let ia = 1.0 - p[3] as f32 / 255.0;
            for j in 0..3 {
                o[j] = (p[j] as f32 + b[j] * ia + 0.5).min(255.0) as u8;
            }
        }
    }

    fn clip_at(&self, x: usize, y: usize) -> f32 {
        self.clips.last().map_or(1.0, |m| m[y * self.w + x])
    }

    /// Blends straight color `c` with coverage `k` into pixel (x, y).
    #[inline]
    fn put(&mut self, x: usize, y: usize, c: Color, k: f32) {
        let k = k * self.opacity * self.clip_at(x, y);
        let sa = c.a * k;
        if sa <= 0.0 {
            return;
        }
        let i = (y * self.w + x) * 4;
        let s = [c.r * sa, c.g * sa, c.b * sa, sa];
        let d = [0, 1, 2, 3].map(|j| self.px[i + j] as f32 / 255.0);
        let da = d[3];
        let out = match self.blend {
            BlendMode::Normal => [0, 1, 2, 3].map(|j| s[j] + d[j] * (1.0 - sa)),
            BlendMode::Add => [0, 1, 2, 3].map(|j| (s[j] + d[j]).min(1.0)),
            BlendMode::Screen => [0, 1, 2, 3].map(|j| s[j] + d[j] - s[j] * d[j]),
            BlendMode::Multiply => [
                s[0] * d[0] + s[0] * (1.0 - da) + d[0] * (1.0 - sa),
                s[1] * d[1] + s[1] * (1.0 - da) + d[1] * (1.0 - sa),
                s[2] * d[2] + s[2] * (1.0 - da) + d[2] * (1.0 - sa),
                sa + da - sa * da,
            ],
            BlendMode::Darken | BlendMode::Lighten => {
                let pick = |a: f32, b: f32| if self.blend == BlendMode::Darken { a.min(b) } else { a.max(b) };
                [
                    pick(s[0] * da, d[0] * sa) + s[0] * (1.0 - da) + d[0] * (1.0 - sa),
                    pick(s[1] * da, d[1] * sa) + s[1] * (1.0 - da) + d[1] * (1.0 - sa),
                    pick(s[2] * da, d[2] * sa) + s[2] * (1.0 - da) + d[2] * (1.0 - sa),
                    sa + da - sa * da,
                ]
            }
        };
        self.px[i..i + 4].copy_from_slice(&out.map(q8));
    }

    /// Rasterizes polygons into the coverage buffer, clipped to the canvas;
    /// returns the box origin.
    fn raster(&mut self, polys: &[Vec<Point>]) -> Option<(usize, usize)> {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in polys {
            for &(x, y) in p {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
        if x0 > x1 {
            return None;
        }
        let bx0 = x0.floor().max(0.0) as usize;
        let by0 = y0.floor().max(0.0) as usize;
        let bx1 = (x1.ceil().max(0.0) as usize).min(self.w);
        let by1 = (y1.ceil().max(0.0) as usize).min(self.h);
        if bx0 >= bx1 || by0 >= by1 {
            return None;
        }
        self.cov.reset(bx1 - bx0, by1 - by0);
        for p in polys {
            self.cov.polygon(p, -(bx0 as f32), -(by0 as f32));
        }
        Some((bx0, by0))
    }

    fn paint_coverage(&mut self, origin: (usize, usize), paint: &Paint) {
        let cov = std::mem::replace(&mut self.cov, Coverage::new());
        cov.for_each(|x, y, k| {
            let (px, py) = (origin.0 + x, origin.1 + y);
            let c = paint.at((px as f32 + 0.5) / self.unit, (py as f32 + 0.5) / self.unit);
            self.put(px, py, c, k);
        });
        self.cov = cov;
    }

    /// Fills `path` (non-zero winding, anti-aliased).
    pub fn fill(&mut self, path: &Path, paint: impl Into<Paint>) {
        let paint = paint.into();
        let polys: Vec<Vec<Point>> = self.sc(path).flatten(0.2).into_iter().map(|(p, _)| p).collect();
        if let Some(o) = self.raster(&polys) {
            self.paint_coverage(o, &paint);
        }
    }

    /// Strokes the outline of `path`.
    pub fn stroke(&mut self, path: &Path, stroke: &Stroke, paint: impl Into<Paint>) {
        let paint = paint.into();
        let st = Stroke { width: stroke.width * self.unit, ..*stroke };
        let polys: Vec<Vec<Point>> = self.sc(path).flatten(0.2).iter().flat_map(|(p, closed)| stroke_polygons(p, *closed, &st)).collect();
        if let Some(o) = self.raster(&polys) {
            self.paint_coverage(o, &paint);
        }
    }

    /// Coverage mask (w × h, 0..1) of a filled path.
    fn mask_of(&mut self, path: &Path) -> Vec<f32> {
        let mut m = vec![0.0; self.w * self.h];
        let polys: Vec<Vec<Point>> = self.sc(path).flatten(0.2).into_iter().map(|(p, _)| p).collect();
        if let Some((ox, oy)) = self.raster(&polys) {
            let w = self.w;
            self.cov.for_each(|x, y, k| m[(oy + y) * w + ox + x] = k);
        }
        m
    }

    /// Draws only inside `path` until [`Canvas::unclip`] (clips nest by
    /// intersection, anti-aliased).
    pub fn clip(&mut self, path: &Path) {
        let mut m = self.mask_of(path);
        if let Some(prev) = self.clips.last() {
            for (a, b) in m.iter_mut().zip(prev) {
                *a *= b;
            }
        }
        self.clips.push(m);
    }

    pub fn unclip(&mut self) {
        self.clips.pop();
    }

    /// Runs `f` with drawing clipped to `path`.
    pub fn clipped(&mut self, path: &Path, f: impl FnOnce(&mut Canvas)) {
        self.clip(path);
        f(self);
        self.unclip();
    }

    /// Draws `f` into a separate layer, then composites it with `opacity`
    /// and `blend` (group opacity: overlapping shapes do not show through
    /// each other).
    pub fn layer(&mut self, opacity: f32, blend: BlendMode, f: impl FnOnce(&mut Canvas)) {
        let saved = std::mem::replace(&mut self.px, vec![0; self.w * self.h * 4]);
        let (so, sb) = (self.opacity, self.blend);
        self.opacity = 1.0;
        self.blend = BlendMode::Normal;
        self.layers.push((saved, opacity, blend));
        f(self);
        let (base, op, bl) = self.layers.pop().expect("layer");
        let layer = std::mem::replace(&mut self.px, base);
        self.opacity = op;
        self.blend = bl;
        for (i, p) in layer.chunks_exact(4).enumerate() {
            if p[3] > 0 {
                let a = p[3] as f32;
                let c = Color { r: p[0] as f32 / a, g: p[1] as f32 / a, b: p[2] as f32 / a, a: a / 255.0 };
                self.put(i % self.w, i / self.w, c, 1.0);
            }
        }
        self.opacity = so;
        self.blend = sb;
    }

    /// Soft shadow of `path`, offset by `(dx, dy)` and blurred by `radius`
    /// (≤ [`MAX_BLUR`]). Draw the shape itself afterwards.
    pub fn shadow(&mut self, path: &Path, dx: f32, dy: f32, radius: u32, color: Color) {
        let mut m = self.mask_of(&path.translate(dx, dy));
        let u = self.unit;
        let (x0, y0, x1, y1) = self.sc(path).bounds();
        let (dx, dy) = (dx * u, dy * u);
        let radius = ((radius.min(MAX_BLUR) as f32) * u).round() as u32;
        let r = radius as f32;
        let rect = (
            (x0 + dx - 3.0 * r).floor().max(0.0) as usize,
            (y0 + dy - 3.0 * r).floor().max(0.0) as usize,
            ((x1 + dx + 3.0 * r).ceil().max(0.0) as usize).min(self.w),
            ((y1 + dy + 3.0 * r).ceil().max(0.0) as usize).min(self.h),
        );
        blur_plane(&mut m, self.w, rect, radius.min(MAX_BLUR) as usize, 1);
        for y in rect.1..rect.3 {
            for x in rect.0..rect.2 {
                let k = m[y * self.w + x];
                if k > 0.0 {
                    self.put(x, y, color, k);
                }
            }
        }
    }

    /// Blurs what is already drawn inside the rectangle (box blur × 3 ≈
    /// Gaussian), radius ≤ [`MAX_BLUR`]. Frosted panels: draw the backdrop,
    /// blur the panel area, then draw a translucent card over it.
    pub fn blur(&mut self, x: usize, y: usize, w: usize, h: usize, radius: u32) {
        let u = self.unit;
        let f = |v: usize| (v as f32 * u).round() as usize;
        let (x, y, w, h, radius) = (f(x), f(y), f(w), f(h), ((radius.min(MAX_BLUR) as f32) * u).round() as u32);
        let (x0, y0, x1, y1) = (x.min(self.w), y.min(self.h), (x + w).min(self.w), (y + h).min(self.h));
        if x1 <= x0 || y1 <= y0 || radius == 0 {
            return;
        }
        // Blur a float copy of the region only.
        let rw = x1 - x0;
        let mut region: Vec<f32> = Vec::with_capacity(rw * (y1 - y0) * 4);
        for yy in y0..y1 {
            region.extend(self.px[(yy * self.w + x0) * 4..(yy * self.w + x1) * 4].iter().map(|&v| v as f32));
        }
        blur_plane(&mut region, rw, (0, 0, rw, y1 - y0), radius.min(MAX_BLUR) as usize, 4);
        for (k, yy) in (y0..y1).enumerate() {
            for (dst, src) in self.px[(yy * self.w + x0) * 4..(yy * self.w + x1) * 4].iter_mut().zip(&region[k * rw * 4..(k + 1) * rw * 4]) {
                *dst = (src + 0.5).clamp(0.0, 255.0) as u8;
            }
        }
    }

    /// Runs a per-pixel shader over the rectangle and blends its output.
    pub fn shade(&mut self, rect: (usize, usize, usize, usize), shader: &dyn super::shader::Shader, u: &super::shader::Uniforms) {
        let (x0, y0) = (rect.0.min(self.w), rect.1.min(self.h));
        let (x1, y1) = ((rect.0 + rect.2).min(self.w), (rect.1 + rect.3).min(self.h));
        let (sx, sy) = (u.size.0 / self.w.max(1) as f32, u.size.1 / self.h.max(1) as f32);
        let direct = self.opacity >= 1.0 && self.blend == BlendMode::Normal && self.clips.is_empty();
        for y in y0..y1 {
            for x in x0..x1 {
                // Shaders see widget coordinates even when rendering at a lower resolution.
                let c = shader.color((x as f32 + 0.5) * sx, (y as f32 + 0.5) * sy, u);
                if direct && c.a >= 1.0 {
                    // Opaque output replaces the pixel: no blending needed.
                    let i = (y * self.w + x) * 4;
                    self.px[i..i + 4].copy_from_slice(&[q8(c.r), q8(c.g), q8(c.b), 255]);
                } else {
                    self.put(x, y, c, 1.0);
                }
            }
        }
    }
}

impl Canvas {
    /// Runs a thread-safe shader over rows `y0..y1` (full width) on up to
    /// `threads` threads, blending with source-over. Used by `View` for
    /// multi-core shading; ignores clip, opacity and blend mode.
    pub(crate) fn shade_rows_parallel(&mut self, y0: usize, y1: usize, shader: &(dyn super::shader::Shader + Sync), u: &super::shader::Uniforms, threads: usize) {
        let (w, y1) = (self.w, y1.min(self.h));
        if y0 >= y1 || w == 0 {
            return;
        }
        let (sx, sy) = (u.size.0 / w as f32, u.size.1 / self.h.max(1) as f32);
        let rows = y1 - y0;
        let per = rows.div_ceil(threads.max(1));
        let u = *u;
        let band = &mut self.px[y0 * w * 4..y1 * w * 4];
        std::thread::scope(|scope| {
            for (k, chunk) in band.chunks_mut(per * w * 4).enumerate() {
                scope.spawn(move || {
                    let ystart = y0 + k * per;
                    for (j, px) in chunk.chunks_exact_mut(4).enumerate() {
                        let (x, y) = (j % w, ystart + j / w);
                        let c = shader.color((x as f32 + 0.5) * sx, (y as f32 + 0.5) * sy, &u);
                        if c.a >= 1.0 {
                            px.copy_from_slice(&[q8(c.r), q8(c.g), q8(c.b), 255]);
                        } else if c.a > 0.0 {
                            let ia = 1.0 - c.a;
                            let d = |v: u8| v as f32 / 255.0;
                            px.copy_from_slice(&[
                                q8(c.r * c.a + d(px[0]) * ia),
                                q8(c.g * c.a + d(px[1]) * ia),
                                q8(c.b * c.a + d(px[2]) * ia),
                                q8(c.a + d(px[3]) * ia),
                            ]);
                        }
                    }
                });
            }
        });
    }
}

impl Canvas {
    /// Runs a portable shader over `rect` (render pixels) on this thread.
    pub(crate) fn shade_sl(&mut self, rect: (usize, usize, usize, usize), sh: &super::sl::Shader, u: &super::shader::Uniforms, ex: &mut super::sl::Exec, row: &mut Vec<Color>) {
        let (x0, y0) = (rect.0.min(self.w), rect.1.min(self.h));
        let (x1, y1) = ((rect.0 + rect.2).min(self.w), (rect.1 + rect.3).min(self.h));
        if x1 <= x0 {
            return;
        }
        let scale = (u.size.0 / self.w.max(1) as f32, u.size.1 / self.h.max(1) as f32);
        row.resize(x1 - x0, Color::TRANSPARENT);
        for y in y0..y1 {
            ex.row(sh, x0 as f32, y as f32, x1 - x0, scale, u, row);
            let w = self.w;
            write_row(&mut self.px[(y * w + x0) * 4..(y * w + x1) * 4], row);
        }
    }

    /// Same on up to `threads` threads, bands of full rows.
    pub(crate) fn shade_sl_rows_parallel(&mut self, y0: usize, y1: usize, sh: &super::sl::Shader, u: &super::shader::Uniforms, threads: usize) {
        let (w, y1) = (self.w, y1.min(self.h));
        if y0 >= y1 || w == 0 {
            return;
        }
        let scale = (u.size.0 / w as f32, u.size.1 / self.h.max(1) as f32);
        let per = (y1 - y0).div_ceil(threads.max(1));
        let u = *u;
        let band = &mut self.px[y0 * w * 4..y1 * w * 4];
        std::thread::scope(|scope| {
            for (k, chunk) in band.chunks_mut(per * w * 4).enumerate() {
                scope.spawn(move || {
                    let mut ex = super::sl::Exec::default();
                    let mut row = vec![Color::TRANSPARENT; w];
                    for (j, line) in chunk.chunks_mut(w * 4).enumerate() {
                        ex.row(sh, 0.0, (y0 + k * per + j) as f32, w, scale, &u, &mut row);
                        write_row(line, &row);
                    }
                });
            }
        });
    }
}

/// Source-over of straight colors onto premultiplied RGBA8 pixels.
fn write_row(px: &mut [u8], row: &[Color]) {
    for (p, c) in px.chunks_exact_mut(4).zip(row) {
        if c.a >= 1.0 {
            p.copy_from_slice(&[q8(c.r), q8(c.g), q8(c.b), 255]);
        } else if c.a > 0.0 {
            let ia = 1.0 - c.a;
            let d = |v: u8| v as f32 / 255.0;
            let out = [q8(c.r * c.a + d(p[0]) * ia), q8(c.g * c.a + d(p[1]) * ia), q8(c.b * c.a + d(p[2]) * ia), q8(c.a + d(p[3]) * ia)];
            p.copy_from_slice(&out);
        }
    }
}

/// Three box-blur passes per axis over `rect` of a plane with `ch` channels.
fn blur_plane(buf: &mut [f32], stride_px: usize, rect: (usize, usize, usize, usize), r: usize, ch: usize) {
    let (x0, y0, x1, y1) = rect;
    if r == 0 || x1 <= x0 || y1 <= y0 {
        return;
    }
    let mut line: Vec<f32> = Vec::new();
    let mut tmp: Vec<f32> = Vec::new();
    for _ in 0..3 {
        for y in y0..y1 {
            line.clear();
            line.extend_from_slice(&buf[(y * stride_px + x0) * ch..(y * stride_px + x1) * ch]);
            box_line(&line, &mut tmp, ch, r);
            buf[(y * stride_px + x0) * ch..(y * stride_px + x1) * ch].copy_from_slice(&tmp);
        }
        for x in x0..x1 {
            line.clear();
            for y in y0..y1 {
                line.extend_from_slice(&buf[(y * stride_px + x) * ch..(y * stride_px + x) * ch + ch]);
            }
            box_line(&line, &mut tmp, ch, r);
            for (k, y) in (y0..y1).enumerate() {
                buf[(y * stride_px + x) * ch..(y * stride_px + x) * ch + ch].copy_from_slice(&tmp[k * ch..k * ch + ch]);
            }
        }
    }
}

/// Moving average of radius `r` along one line (edges clamp).
fn box_line(src: &[f32], dst: &mut Vec<f32>, ch: usize, r: usize) {
    let n = src.len() / ch;
    dst.clear();
    dst.resize(src.len(), 0.0);
    let win = (2 * r + 1) as f32;
    for c in 0..ch {
        let at = |i: isize| src[(i.clamp(0, n as isize - 1) as usize) * ch + c];
        let mut sum: f32 = (-(r as isize)..=r as isize).map(at).sum();
        for i in 0..n {
            dst[i * ch + c] = sum / win;
            sum += at(i as isize + r as isize + 1) - at(i as isize - r as isize);
        }
    }
}

#[inline]
fn q8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx::{Cap, Join, Shader, Uniforms};

    const RED: Color = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
    const BLUE: Color = Color { r: 0.0, g: 0.0, b: 1.0, a: 1.0 };
    const WHITE: Color = Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };

    fn close(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn fill_antialias_and_shapes() {
        let mut c = Canvas::new(20, 20);
        c.fill(&Path::rect(0.5, 0.0, 10.0, 10.0), RED);
        assert!(close(c.pixel(0, 5).a, 0.5, 0.01), "half-covered pixel: {:?}", c.pixel(0, 5));
        assert_eq!(c.pixel(5, 5), RED);
        assert_eq!(c.pixel(15, 15).a, 0.0);
        let mut c = Canvas::new(40, 40);
        c.fill(&Path::rounded_rect(0.0, 0.0, 40.0, 40.0, [12.0, 0.0, 0.0, 0.0]), RED);
        assert_eq!(c.pixel(0, 0).a, 0.0, "rounded corner");
        assert_eq!(c.pixel(39, 0).a, 1.0, "square corner");
        let mut c = Canvas::new(40, 40);
        c.fill(&Path::circle(20.0, 20.0, 10.0), RED);
        let edge = c.pixel(29, 20).a; // the circle edge crosses x = 30
        assert!(edge > 0.4 && edge <= 1.0 && c.pixel(31, 20).a == 0.0, "edge {edge}");
        assert_eq!(c.memory(), 40 * 40 * 4, "4 bytes per pixel");
    }

    #[test]
    fn gradients() {
        let mut c = Canvas::new(101, 3);
        c.fill(&Path::rect(0.0, 0.0, 101.0, 3.0), Paint::linear((0.5, 0.0), (100.5, 0.0), &[(0.0, RED), (1.0, BLUE)]));
        let m = c.pixel(50, 1);
        assert!(close(m.r, 0.5, 0.02) && close(m.b, 0.5, 0.02), "midpoint {m:?}");
        let mut c = Canvas::new(41, 41);
        c.fill(&Path::rect(0.0, 0.0, 41.0, 41.0), Paint::radial((20.5, 20.5), 20.0, &[(0.0, RED), (1.0, BLUE)]));
        assert!(c.pixel(20, 20).r > 0.98 && c.pixel(0, 20).b > 0.98);
        let mut c = Canvas::new(41, 41);
        c.fill(&Path::rect(0.0, 0.0, 41.0, 41.0), Paint::conic((20.5, 20.5), 0.0, &[(0.0, RED), (1.0, BLUE)]));
        // Quarter turn clockwise from +x (screen: below the center) is 25 %.
        let q = c.pixel(20, 35);
        assert!(close(q.b, 0.25, 0.03), "conic quarter {q:?}");
    }

    #[test]
    fn strokes_joins_caps() {
        let line = Path::new().move_to(4.0, 10.0).line_to(16.0, 10.0);
        let mut c = Canvas::new(24, 20);
        c.stroke(&line, &Stroke::new(2.0), RED);
        assert_eq!(c.pixel(10, 9).a, 1.0);
        assert_eq!(c.pixel(10, 10).a, 1.0);
        assert_eq!(c.pixel(10, 12).a, 0.0);
        assert_eq!(c.pixel(17, 10).a, 0.0, "butt cap stops at the end");
        let mut c = Canvas::new(24, 20);
        c.stroke(&line, &Stroke::new(4.0).cap(Cap::Square), RED);
        assert_eq!(c.pixel(17, 10).a, 1.0, "square cap extends by half the width");
        let mut c = Canvas::new(24, 20);
        c.stroke(&line, &Stroke::new(4.0).cap(Cap::Round), RED);
        let r = c.pixel(17, 10).a;
        assert!(r > 0.7 && c.pixel(17, 8).a < r && c.pixel(19, 10).a == 0.0, "round cap: {r}");
        // Miter vs bevel at a right angle: the outer corner pixel.
        let l = Path::new().move_to(4.0, 10.0).line_to(10.0, 10.0).line_to(10.0, 4.0);
        let mut m = Canvas::new(20, 20);
        m.stroke(&l, &Stroke::new(4.0).join(Join::Miter(4.0)), RED);
        let mut b = Canvas::new(20, 20);
        b.stroke(&l, &Stroke::new(4.0).join(Join::Bevel), RED);
        assert_eq!(m.pixel(11, 11).a, 1.0, "miter fills the outer corner");
        assert!(b.pixel(11, 11).a < 0.6, "bevel cuts it: {}", b.pixel(11, 11).a);
        // Overlapping pieces do not cancel each other (closed ring).
        let mut c = Canvas::new(40, 40);
        c.stroke(&Path::circle(20.0, 20.0, 12.0), &Stroke::new(3.0).join(Join::Round), RED);
        assert!(c.pixel(32, 20).a > 0.9 && c.pixel(20, 20).a == 0.0);
    }

    #[test]
    fn clip_layer_blend() {
        let mut c = Canvas::new(40, 40);
        c.clipped(&Path::circle(20.0, 20.0, 10.0), |c| c.fill(&Path::rect(0.0, 0.0, 40.0, 40.0), RED));
        assert_eq!(c.pixel(20, 20), RED);
        assert_eq!(c.pixel(2, 2).a, 0.0, "outside the clip");
        // Group opacity: the overlap is not darker than the rest.
        let mut c = Canvas::new(30, 10);
        c.layer(0.5, BlendMode::Normal, |c| {
            c.fill(&Path::rect(0.0, 0.0, 20.0, 10.0), RED);
            c.fill(&Path::rect(10.0, 0.0, 20.0, 10.0), RED);
        });
        assert!(close(c.pixel(15, 5).a, 0.5, 0.01) && close(c.pixel(5, 5).a, 0.5, 0.01));
        let mut c = Canvas::new(4, 4);
        c.opacity = 0.5;
        c.fill(&Path::rect(0.0, 0.0, 4.0, 4.0), RED);
        assert!(close(c.pixel(1, 1).a, 0.5, 0.01), "global opacity");
        let mut c = Canvas::new(4, 4);
        c.clear(WHITE);
        c.blend = BlendMode::Multiply;
        c.fill(&Path::rect(0.0, 0.0, 4.0, 4.0), RED);
        assert_eq!(c.pixel(1, 1), RED, "white × red = red");
        let mut c = Canvas::new(4, 4);
        c.clear(RED);
        c.blend = BlendMode::Screen;
        c.fill(&Path::rect(0.0, 0.0, 4.0, 4.0), BLUE);
        let p = c.pixel(1, 1);
        assert!(p.r > 0.99 && p.b > 0.99 && p.g < 0.01, "screen red+blue = magenta {p:?}");
        let mut c = Canvas::new(4, 4);
        c.clear(Color::rgba(0.25, 0.0, 0.0, 1.0));
        c.blend = BlendMode::Add;
        c.fill(&Path::rect(0.0, 0.0, 4.0, 4.0), Color::rgba(0.5, 0.0, 0.0, 1.0));
        assert!(close(c.pixel(1, 1).r, 0.75, 0.01), "add");
    }

    #[test]
    fn shadow_blur_shader() {
        let mut c = Canvas::new(60, 60);
        let shape = Path::rect(20.0, 20.0, 20.0, 20.0);
        c.shadow(&shape, 0.0, 6.0, 4, Color::rgba(0.0, 0.0, 0.0, 1.0));
        assert!(c.pixel(30, 44).a > 0.2, "shadow below: {}", c.pixel(30, 44).a);
        assert!(c.pixel(30, 52).a < c.pixel(30, 44).a && c.pixel(30, 59).a < 0.01, "fades out within 3 × radius: {}", c.pixel(30, 59).a);
        assert_eq!(c.pixel(5, 5).a, 0.0);
        let mut c = Canvas::new(40, 10);
        c.fill(&Path::rect(0.0, 0.0, 20.0, 10.0), RED);
        c.blur(0, 0, 40, 10, 4);
        let (l, m, r) = (c.pixel(10, 5).a, c.pixel(20, 5).a, c.pixel(30, 5).a);
        assert!(l > 0.9 && m > 0.3 && m < 0.7 && r < 0.1, "blurred edge {l} {m} {r}");
        let mut c = Canvas::new(8, 8);
        let u = Uniforms { time: 0.0, frame: 0, size: (16.0, 16.0), pointer: None, background: WHITE, foreground: WHITE, accent: RED, urgent: RED, params: [0.0; 4] };
        let s = |x: f32, _y: f32, u: &Uniforms| if x < 8.0 { u.accent } else { BLUE };
        c.shade((0, 0, 8, 8), &s as &dyn Shader, &u);
        assert_eq!(c.pixel(1, 1), RED, "shader sees widget pixels (16 wide) at half resolution");
        assert_eq!(c.pixel(6, 1), BLUE);
    }

    #[test]
    fn parallel_shading_matches_single_thread() {
        let u = Uniforms { time: 0.7, frame: 0, size: (97.0, 61.0), pointer: None, background: WHITE, foreground: WHITE, accent: RED, urgent: RED, params: [0.0; 4] };
        let s = |x: f32, y: f32, u: &Uniforms| u.accent.mix(BLUE, ((x * 0.1).sin() * (y * 0.07 + u.time).cos()).abs()).with_alpha(if x > 80.0 { 0.5 } else { 1.0 });
        let mut a = Canvas::new(97, 61);
        a.clear(Color::rgba(0.2, 0.2, 0.2, 1.0));
        a.shade((0, 0, 97, 61), &s as &dyn Shader, &u);
        for threads in [2, 3, 8] {
            let mut b = Canvas::new(97, 61);
            b.clear(Color::rgba(0.2, 0.2, 0.2, 1.0));
            b.shade_rows_parallel(0, 61, &s, &u, threads);
            assert!(a.px.iter().zip(&b.px).all(|(x, y)| (*x as i16 - *y as i16).abs() <= 1), "{threads} threads");
        }
    }

    #[test]
    fn unit_scale_keeps_widget_coordinates() {
        let mut c = Canvas::new(50, 50);
        c.set_unit(0.5); // a 100×100 widget rendered at 50×50
        c.fill(&Path::rect(50.0, 50.0, 50.0, 50.0), RED);
        assert_eq!(c.pixel(40, 40), RED);
        assert_eq!(c.pixel(10, 10).a, 0.0);
    }
}
