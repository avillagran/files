//! CPU fragment shaders: a function of the pixel and a few uniforms
//! (time, size, pointer, theme palette, four free parameters). They run in
//! tiles inside a [`super::View`] with an area limit and a per-frame time
//! budget, never on the GPU.

use super::canvas::Color;
use crate::theme::Theme;

/// What a shader can read besides the pixel position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Uniforms {
    /// Seconds since the view started animating.
    pub time: f32,
    pub frame: u64,
    /// Widget size in pixels (shader coordinates are widget pixels, even
    /// when the view renders at a lower resolution).
    pub size: (f32, f32),
    /// Mouse position in widget pixels while it is over the view.
    pub pointer: Option<(f32, f32)>,
    pub background: Color,
    pub foreground: Color,
    pub accent: Color,
    pub urgent: Color,
    /// Free parameters (`View::params`).
    pub params: [f32; 4],
}

impl Uniforms {
    /// Uniforms with the colors of `theme`.
    pub fn from_theme(t: &Theme, size: (f32, f32)) -> Uniforms {
        Uniforms {
            time: 0.0,
            frame: 0,
            size,
            pointer: None,
            background: t.palette.background.into(),
            foreground: t.palette.foreground.into(),
            accent: t.palette.accent.into(),
            urgent: t.palette.urgent.into(),
            params: [0.0; 4],
        }
    }
}

/// A per-pixel program: `color(x, y, uniforms)` with `(x, y)` the pixel
/// center in widget pixels. Any `Fn(f32, f32, &Uniforms) -> Color` is one.
pub trait Shader {
    fn color(&self, x: f32, y: f32, u: &Uniforms) -> Color;
}

impl<F: Fn(f32, f32, &Uniforms) -> Color> Shader for F {
    fn color(&self, x: f32, y: f32, u: &Uniforms) -> Color {
        self(x, y, u)
    }
}

// ------------------------------------------------------------ helpers

/// Hash of an integer lattice point to 0..1.
pub fn hash2(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h = (h ^ (h >> 13)).wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0x00ff_ffff) as f32 / 16_777_215.0
}

/// Smooth value noise, about 0..1.
pub fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (xf, yf) = (x - x.floor(), y - y.floor());
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (u, v) = (s(xf), s(yf));
    let a = hash2(xi, yi, seed);
    let b = hash2(xi + 1, yi, seed);
    let c = hash2(xi, yi + 1, seed);
    let d = hash2(xi + 1, yi + 1, seed);
    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

/// Fractal noise (`octaves` layers of value noise), about 0..1.
pub fn fbm(x: f32, y: f32, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 0.5, 1.0, 0.0);
    for o in 0..octaves.max(1) {
        sum += amp * value_noise(x * freq, y * freq, seed.wrapping_add(o * 101));
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

// ------------------------------------------------------------ examples

/// Animated soft noise between the theme background and foreground
/// (`params[0]` = scale in pixels, default 48).
pub fn noise() -> impl Shader {
    |x: f32, y: f32, u: &Uniforms| {
        let scale = if u.params[0] > 0.0 { u.params[0] } else { 48.0 };
        let n = fbm(x / scale + u.time * 0.15, y / scale - u.time * 0.1, 3, 7);
        u.background.mix(u.foreground, n * 0.35)
    }
}

/// Diagonal gradient from the background to the accent that slowly drifts.
pub fn gradient() -> impl Shader {
    |x: f32, y: f32, u: &Uniforms| {
        let (w, h) = (u.size.0.max(1.0), u.size.1.max(1.0));
        let t = (x / w * 0.7 + y / h * 0.3 + 0.25 * (u.time * 0.6).sin()).clamp(0.0, 1.0);
        let mid = u.background.mix(u.accent, 0.55);
        if t < 0.5 { u.background.mix(mid, t * 2.0) } else { mid.mix(u.accent.mix(u.foreground, 0.2), (t - 0.5) * 2.0) }
    }
}

/// Accent glow that breathes at the center, or follows the pointer.
pub fn accent_glow() -> impl Shader {
    |x: f32, y: f32, u: &Uniforms| {
        let (cx, cy) = u.pointer.unwrap_or((u.size.0 / 2.0, u.size.1 / 2.0));
        let r = u.size.0.min(u.size.1) * (0.32 + 0.04 * (u.time * 2.0).sin());
        let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() / r.max(1.0);
        let k = (-d * d * 2.2).exp();
        u.background.mix(u.accent, k * 0.85)
    }
}

/// Moving blobs to put behind a frosted panel (see `View::post` and
/// `Canvas::blur`): the "background blur" demo.
pub fn blobs() -> impl Shader {
    |x: f32, y: f32, u: &Uniforms| {
        let (w, h) = (u.size.0.max(1.0), u.size.1.max(1.0));
        let mut c = u.background;
        for i in 0..3 {
            let fi = i as f32;
            let bx = w * (0.5 + 0.35 * (u.time * (0.5 + 0.2 * fi) + fi * 2.1).sin());
            let by = h * (0.5 + 0.35 * (u.time * (0.4 + 0.15 * fi) + fi * 1.3).cos());
            let d = ((x - bx).powi(2) + (y - by).powi(2)).sqrt() / (w.min(h) * 0.22);
            let tint = match i { 0 => u.accent, 1 => u.foreground, _ => u.urgent };
            c = c.mix(tint, (1.0 - d).clamp(0.0, 1.0) * 0.8);
        }
        c
    }
}
