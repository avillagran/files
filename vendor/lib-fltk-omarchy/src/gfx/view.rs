//! `View`: a widget that shows a [`Canvas`] painted by your closure and/or
//! a shader, optionally animated, with shape hit-testing. The toolkit
//! decides how it is drawn: portable shaders run on the GPU when the app
//! was built with the `gpu` feature and a GL context works, and on the
//! CPU otherwise. The app's code is the same either way.

use super::canvas::{Canvas, Color};
use super::path::Path;
use super::raster::Stroke;
use super::shader::{Shader, Uniforms};
use super::sl;
use crate::theme;
use fltk::{app, enums::*, group::Group, image::RgbImage, prelude::*};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

type PaintFn = Box<dyn FnMut(&mut Canvas, &Uniforms)>;

/// Where a View renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// The toolkit decides (default): GPU when available, else CPU.
    Auto,
    /// Always the CPU (software).
    Cpu,
    /// The GPU when possible (falls back to the CPU and says why).
    Gpu,
}

impl Backend {
    pub fn parse(s: &str) -> Option<Backend> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Backend::Auto),
            "cpu" | "software" => Some(Backend::Cpu),
            "gpu" | "gl" => Some(Backend::Gpu),
            _ => None,
        }
    }
}

/// How often an animated View draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rate {
    /// The refresh rate of the monitor showing the window on the GPU;
    /// capped on the CPU (see [`set_cpu_fps_cap`], default 30).
    Display,
    /// This many frames per second (0 stops).
    Fixed(u32),
}

impl From<u32> for Rate {
    fn from(fps: u32) -> Rate {
        Rate::Fixed(fps)
    }
}

thread_local! {
    static DEFAULT_BACKEND: Cell<Option<Backend>> = const { Cell::new(None) };
    static CPU_CAP: Cell<u32> = const { Cell::new(30) };
}

/// Backend for Views that do not set one: `OMARCHY_FLTK_RENDER=auto|cpu|gpu`
/// or [`set_backend`]; default `Auto`.
pub fn default_backend() -> Backend {
    DEFAULT_BACKEND.with(|b| {
        b.get().unwrap_or_else(|| {
            let v = std::env::var("OMARCHY_FLTK_RENDER").ok().and_then(|s| Backend::parse(&s)).unwrap_or(Backend::Auto);
            b.set(Some(v));
            v
        })
    })
}

pub fn set_backend(b: Backend) {
    DEFAULT_BACKEND.with(|c| c.set(Some(b)));
}

/// Largest fps of `Rate::Display` on the CPU (each CPU frame costs CPU time).
pub fn set_cpu_fps_cap(fps: u32) {
    CPU_CAP.with(|c| c.set(fps.max(1)));
}

fn cpu_cap() -> u32 {
    CPU_CAP.with(|c| c.get())
}

/// The shape that receives mouse events (outside it, events go to what
/// is below, as if the view were not there).
#[derive(Debug, Clone, PartialEq)]
pub enum Hit {
    /// Inside the filled path.
    Fill(Path),
    /// Within `width / 2` of the outline.
    Outline(Path, f32),
}

impl Hit {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        match self {
            Hit::Fill(p) => p.contains(x, y),
            Hit::Outline(p, w) => p.near_outline(x, y, *w),
        }
    }
}

/// Render counters, for budgets and measurements.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ViewStats {
    /// Completed frames.
    pub frames: u64,
    /// Draws that ran out of budget and continued on the next draw (CPU).
    pub partial_draws: u64,
    /// CPU time of the last completed frame, in ms (on the GPU: building
    /// the overlay and submitting).
    pub last_frame_ms: f32,
    /// Resolution actually rendered (≤ widget size, see `max_area`).
    pub render_size: (usize, usize),
    /// Animation ticks skipped because the view was hidden.
    pub paused_ticks: u64,
    /// Bytes held by the view's CPU buffers.
    pub memory: usize,
    /// Last draw: converting the frame for fltk and drawing it, in ms (CPU).
    pub last_blit_ms: f32,
    /// Frames per second of the animation timer right now.
    pub target_fps: f32,
}

const TILE: usize = 32;

#[derive(Clone)]
enum Kind {
    Portable(sl::Shader),
    Cpu(Arc<dyn Shader + Send + Sync>),
}

struct State {
    present: Canvas,
    work: Canvas,
    upload: Vec<u8>,
    paint: Option<PaintFn>,
    post: Option<PaintFn>,
    shader: Option<Kind>,
    exec: sl::Exec,
    row: Vec<Color>,
    threads: usize,
    rate: Rate,
    backend: Option<Backend>,
    gpu: bool,
    reason: String,
    hz: f64,
    hz_at: Option<Instant>,
    params: [f32; 4],
    started: Instant,
    frozen: Option<f32>,
    frame: u64,
    pointer: Option<(f32, f32)>,
    max_area: usize,
    budget: Duration,
    next_tile: usize,
    in_progress: Option<(Uniforms, Duration)>,
    dirty: bool,
    hit: Option<Hit>,
    pressed: bool,
    click: Option<Box<dyn FnMut()>>,
    stats: ViewStats,
    timer: bool,
    #[cfg_attr(not(feature = "gpu"), allow(dead_code))]
    shown_ticks: u32,
    /// GPU overlay state: (had paint or ring, ring shown).
    #[cfg_attr(not(feature = "gpu"), allow(dead_code))]
    overlay_was: (bool, bool),
    #[cfg(feature = "gpu")]
    layer: Option<super::gpu::GlLayer>,
}

/// A drawing surface: vector content, shaders, animation, shaped hit area.
///
/// ```no_run
/// use fltk_omarchy::gfx::{self, Path, Paint, Rate, Stroke, View};
/// let card = View::new(240, 120).paint(|c, u| {
///     let shape = Path::rounded_rect(8.0, 8.0, 224.0, 104.0, [16.0, 16.0, 4.0, 4.0]);
///     c.shadow(&shape, 0.0, 4.0, 8, u.foreground.with_alpha(0.35));
///     c.fill(&shape, Paint::linear((0.0, 0.0), (240.0, 0.0), &[(0.0, u.accent), (1.0, u.background)]));
///     c.stroke(&shape, &Stroke::new(1.5), u.foreground.with_alpha(0.4));
/// });
/// let glow = View::new(240, 120).shader(gfx::shaders::accent_glow()).animate(Rate::Display);
/// ```
///
/// - `paint(f)` draws vector content (redrawn on theme change or `invalidate()`).
/// - `shader(s)` fills every pixel with a portable [`sl::Shader`]: GPU when
///   available, CPU otherwise; `cpu_shader(f)` for a Rust closure (CPU only).
/// - `post(f)` runs on the CPU shader output (blur a region…); a View with
///   `post` renders on the CPU.
/// - `animate(Rate::Display)` follows the monitor on the GPU (capped on
///   the CPU); `animate(30)` is a fixed rate. Hidden views skip ticks.
/// - CPU limits: `max_area(px)` (default 160 000 px), `budget_ms(ms)` per
///   draw (default 6 ms), `threads(n)`.
#[derive(Clone)]
pub struct View {
    group: Group,
    st: Rc<RefCell<State>>,
}
fltk::widget_extends!(View, Group, group);

impl View {
    pub fn new(w: i32, h: i32) -> View {
        let mut group = Group::default().with_size(w, h);
        group.set_frame(FrameType::NoBox);
        group.end();
        group.clear_visible_focus();
        crate::focus::control_group(&group);
        let st = Rc::new(RefCell::new(State {
            present: Canvas::new(0, 0),
            work: Canvas::new(0, 0),
            upload: Vec::new(),
            paint: None,
            post: None,
            shader: None,
            exec: sl::Exec::default(),
            row: Vec::new(),
            threads: 1,
            rate: Rate::Fixed(0),
            backend: None,
            gpu: false,
            reason: String::new(),
            hz: 60.0,
            hz_at: None,
            params: [0.0; 4],
            started: Instant::now(),
            frozen: None,
            frame: 0,
            pointer: None,
            max_area: 160_000,
            budget: Duration::from_millis(6),
            next_tile: 0,
            in_progress: None,
            dirty: true,
            hit: None,
            pressed: false,
            click: None,
            stats: ViewStats::default(),
            timer: false,
            shown_ticks: 0,
            overlay_was: (false, false),
            #[cfg(feature = "gpu")]
            layer: None,
        }));
        let s2 = st.clone();
        group.draw(move |g| draw(g, &s2));
        // cfltk runs FLTK's own draw before this closure by default: it would
        // draw damaged children first and this background would then cover them.
        group.super_draw(false);
        let s3 = st.clone();
        group.handle(move |g, ev| {
            let (lx, ly) = ((app::event_x() - g.x()) as f32 + 0.5, (app::event_y() - g.y()) as f32 + 0.5);
            handle(g, ev, &s3, lx, ly)
        });
        let s4 = st.clone();
        let mut g2 = group.clone();
        theme::on_change_while(&group, move |_| {
            s4.borrow_mut().dirty = true;
            g2.redraw();
        });
        let v = View { group, st };
        v.start_timer();
        v
    }

    /// Vector content: `f(canvas, uniforms)`, in widget pixels.
    pub fn paint(self, f: impl FnMut(&mut Canvas, &Uniforms) + 'static) -> Self {
        self.st.borrow_mut().paint = Some(Box::new(f));
        self.invalidate();
        self
    }

    /// Fills every pixel with a portable shader (opaque over the theme
    /// background); `paint` draws on top. Runs on the GPU or the CPU.
    pub fn shader(self, shader: sl::Shader) -> Self {
        self.st.borrow_mut().shader = Some(Kind::Portable(shader));
        self.invalidate();
        self
    }

    /// A Rust closure shader: always on the CPU (for effects `sl` cannot
    /// express). Must be thread-safe so `threads(n)` can split it.
    pub fn cpu_shader(self, shader: impl Shader + Send + Sync + 'static) -> Self {
        self.st.borrow_mut().shader = Some(Kind::Cpu(Arc::new(shader)));
        self.invalidate();
        self
    }

    /// Runs on the shader output before it is shown (e.g. `c.blur(..)`).
    /// Needs the pixels on the CPU: the View then renders on the CPU.
    pub fn post(self, f: impl FnMut(&mut Canvas, &Uniforms) + 'static) -> Self {
        self.st.borrow_mut().post = Some(Box::new(f));
        self.invalidate();
        self
    }

    /// Animates: `animate(Rate::Display)` or `animate(30)`; `animate(0)` stops.
    pub fn animate(self, rate: impl Into<Rate>) -> Self {
        self.st.borrow_mut().rate = rate.into();
        self.start_timer();
        self
    }

    /// Forces a backend for this View (default: [`default_backend`]).
    pub fn backend(self, b: Backend) -> Self {
        self.st.borrow_mut().backend = Some(b);
        self.invalidate();
        self
    }

    /// The backend in use now.
    pub fn active_backend(&self) -> Backend {
        if self.st.borrow().gpu { Backend::Gpu } else { Backend::Cpu }
    }

    /// Why the View is on the CPU when the GPU was possible (empty otherwise).
    pub fn backend_reason(&self) -> String {
        self.st.borrow().reason.clone()
    }

    /// CPU shading on `n` threads (bands of rows; 0 = the machine's cores,
    /// at most 8). Default 1.
    pub fn threads(self, n: usize) -> Self {
        let n = if n == 0 { std::thread::available_parallelism().map_or(1, |c| c.get()).min(8) } else { n.min(8) };
        self.st.borrow_mut().threads = n.max(1);
        self
    }

    /// Largest number of pixels rendered per CPU frame (minimum 1024).
    pub fn max_area(self, px: usize) -> Self {
        self.st.borrow_mut().max_area = px.max(1024);
        self.invalidate();
        self
    }

    /// CPU time per draw before a CPU shader frame continues on the next draw.
    pub fn budget_ms(self, ms: f32) -> Self {
        self.st.borrow_mut().budget = Duration::from_secs_f32(ms.max(0.5) / 1000.0);
        self
    }

    /// Free shader parameters (`u.param(i)` / `Uniforms::params`).
    pub fn set_params(&self, p: [f32; 4]) {
        self.st.borrow_mut().params = p;
        self.invalidate();
    }

    /// Freezes the shader time (screenshots, tests); None follows the clock.
    pub fn set_time(&self, t: Option<f32>) {
        self.st.borrow_mut().frozen = t;
        self.invalidate();
    }

    /// Only this shape gets mouse events; with `on_click` it is also a
    /// keyboard control (Tab, Enter/Space) with a focus ring along the shape.
    pub fn hit_shape(self, hit: Hit) -> Self {
        self.st.borrow_mut().hit = Some(hit);
        self
    }

    pub fn on_click(mut self, f: impl FnMut() + 'static) -> Self {
        self.st.borrow_mut().click = Some(Box::new(f));
        self.group.set_visible_focus();
        self
    }

    /// Asks for a new frame.
    pub fn invalidate(&self) {
        self.st.borrow_mut().dirty = true;
        self.group.clone().redraw();
    }

    /// Finishes the pending CPU frame now, ignoring the budget (tests, snapshots).
    pub fn render_now(&self) {
        let (w, h) = (self.group.w(), self.group.h());
        render(&self.st, w, h, Duration::MAX);
    }

    pub fn stats(&self) -> ViewStats {
        #[allow(unused_mut)]
        let mut s = self.st.borrow().stats;
        #[cfg(feature = "gpu")]
        if let Some(l) = &self.st.borrow().layer {
            let ls = l.st.borrow();
            s.frames = ls.frames;
            s.last_frame_ms = ls.last_draw_ms;
        }
        s
    }

    /// The last complete CPU frame (render resolution).
    pub fn with_canvas<R>(&self, f: impl FnOnce(&Canvas) -> R) -> R {
        f(&self.st.borrow().present)
    }

    /// Reads a pixel of the GPU output at widget `(x, y)` after the next
    /// GPU draw (tests). Returns false on the CPU backend.
    pub fn read_gpu_pixel(&self, _x: i32, _y: i32, _f: impl FnOnce([u8; 4]) + 'static) -> bool {
        #[cfg(feature = "gpu")]
        if let Some(l) = &self.st.borrow().layer {
            l.read_pixel(_x, _y, Box::new(_f));
            l.win.clone().redraw();
            return true;
        }
        false
    }

    /// The timer: animation ticks, display-rate tracking and backend choice.
    fn start_timer(&self) {
        if std::mem::replace(&mut self.st.borrow_mut().timer, true) {
            return;
        }
        let (st, g) = (Rc::downgrade(&self.st), self.group.clone());
        app::add_timeout3(0.05, move |h| {
            let Some(st) = st.upgrade() else { return };
            if g.was_deleted() {
                return;
            }
            let shown = g.visible_r() && g.window().is_some_and(|win| win.shown());
            choose_backend(&g, &st, shown);
            let fps = target_fps(&g, &st);
            st.borrow_mut().stats.target_fps = fps as f32;
            let animated = fps > 0.0;
            if shown && animated {
                // CPU: a new frame. GPU: only the shader time moves; the
                // overlay is rebuilt when it changes (see draw_gpu).
                let gpu = st.borrow().gpu;
                if gpu {
                    // Only the GL layer redraws; an animated paint() still
                    // needs the overlay, which the group draw rebuilds.
                    #[cfg(feature = "gpu")]
                    {
                        let paints = st.borrow().paint.is_some();
                        if paints {
                            g.clone().redraw();
                        } else {
                            push_layer_frame(&g, &st);
                        }
                    }
                } else {
                    st.borrow_mut().dirty = true;
                    g.clone().redraw();
                }
                app::repeat_timeout3(1.0 / fps, h);
            } else {
                if animated {
                    st.borrow_mut().stats.paused_ticks += 1;
                }
                // Idle: a slow tick keeps the backend choice and the display
                // rate current; a static View draws only when invalidated.
                app::repeat_timeout3(if shown { 0.25 } else { 0.5 }, h);
            }
        });
    }
}

/// Animation fps now (0 = not animated).
fn target_fps(g: &Group, st: &Rc<RefCell<State>>) -> f64 {
    let (rate, gpu) = { let s = st.borrow(); (s.rate, s.gpu) };
    match rate {
        Rate::Fixed(n) => n as f64,
        Rate::Display => {
            let stale = st.borrow().hz_at.is_none_or_elapsed(Duration::from_secs(3));
            if stale {
                let title = g.window().map(|w| w.label());
                let hz = crate::hyprland::refresh_hz_for(title.as_deref()).unwrap_or(60.0);
                let mut s = st.borrow_mut();
                s.hz = hz;
                s.hz_at = Some(Instant::now());
            }
            let hz = st.borrow().hz;
            if gpu { hz } else { hz.min(cpu_cap() as f64) }
        }
    }
}

trait Stale {
    fn is_none_or_elapsed(&self, d: Duration) -> bool;
}

impl Stale for Option<Instant> {
    fn is_none_or_elapsed(&self, d: Duration) -> bool {
        self.map_or(true, |t| t.elapsed() >= d)
    }
}

/// Picks GPU or CPU for this View (runs on the timer).
fn choose_backend(g: &Group, st: &Rc<RefCell<State>>, shown: bool) {
    let want = st.borrow().backend.unwrap_or_else(default_backend);
    let (portable, post) = {
        let s = st.borrow();
        (matches!(s.shader, Some(Kind::Portable(_))), s.post.is_some())
    };
    let reason = if want == Backend::Cpu {
        ""
    } else if !portable {
        "no portable shader (cpu_shader or paint only)"
    } else if post {
        "post() needs the pixels on the CPU"
    } else if !cfg!(feature = "gpu") {
        "built without the `gpu` feature"
    } else {
        ""
    };
    st.borrow_mut().reason = reason.to_string();
    #[cfg(not(feature = "gpu"))]
    let _ = (g, shown);
    #[cfg(feature = "gpu")]
    {
        let eligible = want != Backend::Cpu && portable && !post;
        let has_layer = st.borrow().layer.is_some();
        if !eligible && has_layer {
            drop_layer(g, st);
            return;
        }
        if has_layer {
            // A GL error after the first draw: back to the CPU, say why.
            let err = st.borrow().layer.as_ref().and_then(|l| l.st.borrow().error.clone());
            if let Some(e) = err {
                drop_layer(g, st);
                st.borrow_mut().reason = format!("GPU shader failed: {}", e.lines().next().unwrap_or(""));
                st.borrow_mut().backend = Some(Backend::Cpu);
            }
            return;
        }
        if !eligible {
            return;
        }
        // FLTK's Wayland GL subwindow needs its window on an output: start
        // it only after the window has been shown for a few ticks.
        let ticks = if shown { st.borrow().shown_ticks + 1 } else { 0 };
        st.borrow_mut().shown_ticks = ticks;
        if ticks < 3 {
            return;
        }
        let glsl = match &st.borrow().shader {
            Some(Kind::Portable(s)) => s.glsl().to_string(),
            _ => return,
        };
        let (s2, mut g2) = (Rc::downgrade(st), g.clone());
        let grp = g.clone();
        grp.begin();
        let layer = super::gpu::GlLayer::new(g.x(), g.y(), g.w(), g.h(), &glsl, move |ev, x, y| {
            let Some(st) = s2.upgrade() else { return false };
            handle(&mut g2, ev, &st, x as f32 + 0.5, y as f32 + 0.5)
        });
        grp.end();
        grp.resizable(&layer.win);
        let mut lw = layer.win.clone();
        lw.show();
        {
            let mut s = st.borrow_mut();
            s.layer = Some(layer);
            s.gpu = true;
            s.dirty = true;
        }
        g.clone().redraw();
    }
}

#[cfg(feature = "gpu")]
fn drop_layer(g: &Group, st: &Rc<RefCell<State>>) {
    let layer = st.borrow_mut().layer.take();
    if let Some(l) = layer {
        let mut grp = g.clone();
        grp.remove(&l.win);
        <fltk::window::GlWindow as WidgetBase>::delete(l.win);
    }
    let mut s = st.borrow_mut();
    s.gpu = false;
    s.dirty = true;
    drop(s);
    g.clone().redraw();
}

fn render_size(w: i32, h: i32, max_area: usize) -> (usize, usize) {
    let (w, h) = (w.max(1) as usize, h.max(1) as usize);
    let area = w * h;
    if area <= max_area {
        return (w, h);
    }
    let s = (max_area as f64 / area as f64).sqrt();
    (((w as f64 * s) as usize).max(1), ((h as f64 * s) as usize).max(1))
}

fn uniforms(s: &mut State, w: i32, h: i32) -> Uniforms {
    let mut u = theme::with(|t| Uniforms::from_theme(t, (w as f32, h as f32)));
    u.time = s.frozen.unwrap_or_else(|| s.started.elapsed().as_secs_f32());
    u.frame = s.frame;
    u.pointer = s.pointer;
    u.params = s.params;
    u
}

/// Advances the CPU frame; returns true when a frame is complete.
fn render(st: &Rc<RefCell<State>>, w: i32, h: i32, budget: Duration) -> bool {
    let mut s = st.borrow_mut();
    let (rw, rh) = render_size(w, h, s.max_area);
    if (s.present.width(), s.present.height()) != (rw, rh) {
        s.present.resize(rw, rh);
        s.work.resize(rw, rh);
        if s.upload.capacity() > 2 * rw * rh * 4 + 4096 {
            s.upload = Vec::new();
        }
        let unit = rw as f32 / w.max(1) as f32;
        s.present.set_unit(unit);
        s.work.set_unit(unit);
        s.in_progress = None;
        s.dirty = true;
    }
    if !s.dirty && s.in_progress.is_none() {
        return true;
    }
    let t0 = Instant::now();
    if s.in_progress.is_none() {
        let u = uniforms(&mut s, w, h);
        s.frame += 1;
        s.dirty = false;
        s.next_tile = 0;
        s.work.clear(Color::TRANSPARENT);
        s.in_progress = Some((u, Duration::ZERO));
    }
    let (u, spent) = s.in_progress.expect("frame");
    let first_frame = s.stats.frames == 0;
    // Shader tiles, until done or out of budget (the first frame always completes).
    if let Some(kind) = s.shader.clone() {
        let threads = s.threads;
        let (cols, rows) = (rw.div_ceil(TILE), rh.div_ceil(TILE));
        let total = if threads > 1 { rows } else { cols * rows };
        while s.next_tile < total {
            let t = s.next_tile;
            let State { work, exec, row, .. } = &mut *s;
            if threads > 1 {
                let bands = threads.min(total - t);
                match &kind {
                    Kind::Portable(sh) => work.shade_sl_rows_parallel(t * TILE, (t + bands) * TILE, sh, &u, threads),
                    Kind::Cpu(sh) => work.shade_rows_parallel(t * TILE, (t + bands) * TILE, sh.as_ref(), &u, threads),
                }
                s.next_tile += bands;
            } else {
                let rect = ((t % cols) * TILE, (t / cols) * TILE, TILE, TILE);
                match &kind {
                    Kind::Portable(sh) => work.shade_sl(rect, sh, &u, exec, row),
                    Kind::Cpu(sh) => work.shade(rect, sh.as_ref(), &u),
                }
                s.next_tile += 1;
            }
            if !first_frame && t0.elapsed() >= budget {
                break;
            }
        }
        if s.next_tile < total {
            s.in_progress = Some((u, spent + t0.elapsed()));
            s.stats.partial_draws += 1;
            return false;
        }
    }
    let State { work, post, paint, .. } = &mut *s;
    if let Some(f) = post.as_mut() {
        f(work, &u);
    }
    if let Some(f) = paint.as_mut() {
        f(work, &u);
    }
    let s = &mut *s;
    std::mem::swap(&mut s.present, &mut s.work);
    s.in_progress = None;
    s.stats.frames += 1;
    s.stats.last_frame_ms = (spent + t0.elapsed()).as_secs_f32() * 1000.0;
    s.stats.render_size = (rw, rh);
    s.stats.memory = s.present.memory() + s.work.memory() + s.upload.capacity();
    true
}

/// Focus ring along the hit shape (or the bounds), in widget pixels.
fn draw_ring(c: &mut Canvas, hit: &Option<Hit>, w: i32, h: i32) {
    let (ring, halo) = theme::with(|t| (t.border(&t.controls.focus, t.palette.background), t.palette.background));
    let shape = match hit {
        Some(Hit::Fill(p)) | Some(Hit::Outline(p, _)) => p.clone(),
        None => Path::rect(1.0, 1.0, w as f32 - 2.0, h as f32 - 2.0),
    };
    // A background halo under the ring keeps it visible over any fill.
    c.stroke(&shape, &Stroke::new(6.0), Color::from(halo));
    c.stroke(&shape, &Stroke::new(2.5), ring);
}

fn draw(g: &mut Group, st: &Rc<RefCell<State>>) {
    #[cfg(feature = "gpu")]
    if st.borrow().gpu {
        draw_gpu(g, st);
        g.draw_children();
        return;
    }
    let budget = st.borrow().budget;
    let done = render(st, g.w(), g.h(), budget);
    if !done {
        let mut g2 = g.clone();
        app::add_timeout3(0.0, move |_| if !g2.was_deleted() { g2.redraw() });
    }
    let t_blit = Instant::now();
    let mut s = st.borrow_mut();
    let s = &mut *s;
    let (rw, rh) = (s.present.width() as i32, s.present.height() as i32);
    if rw == 0 || rh == 0 {
        return;
    }
    let opaque = s.shader.is_some();
    // SAFETY: `img` borrows `s.upload` and is dropped at the end of this
    // block; the buffer is not touched while it lives.
    unsafe {
        let img = if opaque {
            let bg: Color = theme::with(|t| t.palette.background.into());
            s.present.to_rgb8_over(bg, &mut s.upload);
            RgbImage::from_data(&s.upload, rw, rh, ColorDepth::Rgb8)
        } else {
            s.present.to_rgba8(&mut s.upload);
            RgbImage::from_data(&s.upload, rw, rh, ColorDepth::Rgba8)
        };
        if let Ok(mut img) = img {
            if (rw, rh) != (g.w(), g.h()) {
                img.scale(g.w(), g.h(), false, true);
            }
            img.draw(g.x(), g.y(), g.w(), g.h());
        }
    }
    s.stats.last_blit_ms = t_blit.elapsed().as_secs_f32() * 1000.0;
    if crate::focus::show_ring(g) {
        let mut c = Canvas::new(g.w().max(1) as usize, g.h().max(1) as usize);
        draw_ring(&mut c, &s.hit, g.w(), g.h());
        let mut buf = Vec::new();
        c.to_rgba8(&mut buf);
        if let Ok(mut img) = RgbImage::new(&buf, g.w(), g.h(), ColorDepth::Rgba8) {
            img.draw(g.x(), g.y(), g.w(), g.h());
        }
    }
}

/// GPU backend: the shader runs in the GL layer; `paint` and the focus ring
/// are drawn on the CPU into an overlay texture when they change.
#[cfg(feature = "gpu")]
fn draw_gpu(g: &mut Group, st: &Rc<RefCell<State>>) {
    let t0 = Instant::now();
    let (w, h) = (g.w(), g.h());
    let ring = crate::focus::show_ring(g);
    let mut s = st.borrow_mut();
    let u = uniforms(&mut s, w, h);
    s.frame += 1;
    let animated_paint = s.paint.is_some() && matches!(s.rate, Rate::Display | Rate::Fixed(1..));
    let has = s.paint.is_some() || ring;
    let changed = s.dirty || animated_paint || (has, ring) != s.overlay_was;
    s.overlay_was = (has, ring);
    if changed && !has {
        // Nothing on top of the shader: drop the overlay once.
        s.dirty = false;
        if let Some(l) = s.layer.as_ref() {
            l.set_overlay(&[], 0, 0);
        }
    } else if changed {
        s.dirty = false;
        if (s.present.width(), s.present.height()) != (w.max(1) as usize, h.max(1) as usize) {
            s.present.resize(w.max(1) as usize, h.max(1) as usize);
            s.present.set_unit(1.0);
        }
        s.present.clear(Color::TRANSPARENT);
        let State { present, paint, hit, .. } = &mut *s;
        if let Some(f) = paint.as_mut() {
            f(present, &u);
        }
        if ring {
            draw_ring(present, hit, w, h);
        }
        let State { present, layer, upload, .. } = &mut *s;
        if let Some(l) = layer.as_ref() {
            // Premultiplied RGBA8 straight from the canvas.
            upload.clear();
            present.premul_bytes(upload);
            l.set_overlay(upload, w, h);
        }
    }
    drop(s);
    push_layer_frame(g, st);
    let mut s = st.borrow_mut();
    s.stats.render_size = (w.max(1) as usize, h.max(1) as usize);
    s.stats.memory = s.present.memory() + s.upload.capacity();
    let _ = t0;
}

/// Hands the current time, params and pointer to the GL layer and redraws it.
#[cfg(feature = "gpu")]
fn push_layer_frame(g: &Group, st: &Rc<RefCell<State>>) {
    let mut s = st.borrow_mut();
    let u = uniforms(&mut s, g.w(), g.h());
    if let Some(l) = s.layer.as_ref() {
        l.st.borrow_mut().frame = super::gpu::LayerFrame { time: u.time, params: u.params, pointer: u.pointer };
        l.win.clone().redraw();
    }
}

fn handle(g: &mut Group, ev: Event, st: &Rc<RefCell<State>>, lx: f32, ly: f32) -> bool {
    let inside = |st: &State| st.hit.as_ref().map_or(true, |h| h.contains(lx, ly));
    match ev {
        Event::Enter | Event::Move => {
            let mut s = st.borrow_mut();
            s.pointer = Some((lx, ly));
            if s.shader.is_some() && !matches!(s.rate, Rate::Display | Rate::Fixed(1..)) {
                s.dirty = true;
                drop(s);
                g.redraw();
            }
            true
        }
        Event::Leave => {
            st.borrow_mut().pointer = None;
            false
        }
        Event::Push => {
            let s = st.borrow();
            if s.hit.is_some() && !inside(&s) {
                return false; // let the widget below get it
            }
            if s.click.is_none() {
                return false;
            }
            drop(s);
            st.borrow_mut().pressed = true;
            let _ = g.take_focus();
            true
        }
        Event::Released => {
            let ok = {
                let mut s = st.borrow_mut();
                std::mem::replace(&mut s.pressed, false) && inside(&s)
            };
            if ok {
                fire(st);
            }
            ok
        }
        Event::Focus | Event::Unfocus => {
            let focusable = st.borrow().click.is_some();
            if focusable {
                st.borrow_mut().dirty = true;
                g.redraw();
            }
            focusable
        }
        Event::KeyDown if crate::focus::is_focused(g) => {
            let k = app::event_key();
            if k == Key::Enter || k == Key::KPEnter || k == Key::from_char(' ') {
                fire(st);
                return true;
            }
            false
        }
        _ => false,
    }
}

fn fire(st: &Rc<RefCell<State>>) {
    let f = st.borrow_mut().click.take();
    if let Some(mut f) = f {
        f();
        let mut s = st.borrow_mut();
        if s.click.is_none() {
            s.click = Some(f);
        }
    }
}
