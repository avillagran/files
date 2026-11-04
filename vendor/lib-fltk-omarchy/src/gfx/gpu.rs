//! GPU shaders (crate feature `gpu`, **off by default**).
//!
//! `GpuView` is an OpenGL subwindow that runs a GLSL fragment shader with
//! the same uniforms as the CPU [`super::Shader`]: time, size, pointer,
//! the theme palette and four params. Enabling the feature turns on fltk's
//! `enable-glwindow` (fltk-sys builds with OpenGL) and adds the `glow`
//! crate; apps that do not enable it build exactly as without this module.
//! Cost and limits: docs/SHADERS.md.
//!
//! ```no_run
//! # #[cfg(feature = "gpu")] {
//! use fltk_omarchy::gfx::gpu::GpuView;
//! let glow = GpuView::new(400, 300, r#"
//!     vec4 shade(vec2 p) {
//!         float d = length(p - u_size * 0.5) / min(u_size.x, u_size.y);
//!         return mix(u_accent, u_background, smoothstep(0.0, 0.5, d));
//!     }
//! "#).animate(60);
//! # }
//! ```
//!
//! The fragment code defines `vec4 shade(vec2 p)`; `p` is in widget pixels
//! with y down, like the CPU shaders. Uniforms: `float u_time`,
//! `vec2 u_size`, `vec2 u_pointer` (−1 when outside), `vec4 u_background`,
//! `u_foreground`, `u_accent`, `u_urgent`, `u_params`.

use crate::theme;
use fltk::{app, enums::*, prelude::*, window::GlWindow};
use glow::HasContext;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

const PRELUDE: &str = "#version 330 core
uniform float u_time;
uniform vec2 u_size;
uniform vec2 u_pointer;
uniform float u_scale;
uniform vec4 u_background;
uniform vec4 u_foreground;
uniform vec4 u_accent;
uniform vec4 u_urgent;
uniform vec4 u_params;
out vec4 o_color;
vec4 shade(vec2 p);
void main() {
    vec2 p = vec2(gl_FragCoord.x, u_size.y * u_scale - gl_FragCoord.y) / u_scale;
    o_color = shade(p);
}
#line 1
";

const VERTEX: &str = "#version 330 core
void main() {
    vec2 v = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
    gl_Position = vec4(v * 2.0 - 1.0, 0.0, 1.0);
}
";

/// Counters of a [`GpuView`].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GpuStats {
    pub frames: u64,
    /// CPU time of the last draw call (submitting, not GPU time), in ms.
    pub last_draw_ms: f32,
    pub paused_ticks: u64,
    /// Framebuffer size of the last draw.
    pub pixel_size: (i32, i32),
}

struct Gl {
    ctx: glow::Context,
    program: glow::Program,
    vao: glow::VertexArray,
}

struct State {
    source: String,
    gl: Option<Gl>,
    error: Option<String>,
    fps: u32,
    started: Instant,
    frozen: Option<f32>,
    params: [f32; 4],
    pointer: Option<(f32, f32)>,
    stats: GpuStats,
    timer: bool,
    readback: Vec<(i32, i32, Box<dyn FnOnce([u8; 4])>)>,
}

/// An OpenGL subwindow running a GLSL fragment shader.
#[derive(Clone)]
pub struct GpuView {
    win: GlWindow,
    st: Rc<RefCell<State>>,
}
fltk::widget_extends!(GpuView, GlWindow, win);

impl GpuView {
    /// `fragment` defines `vec4 shade(vec2 p)`. Compile errors are reported
    /// by [`GpuView::last_error`] after the first draw; the view then shows
    /// the theme background.
    pub fn new(w: i32, h: i32, fragment: &str) -> GpuView {
        let mut win = GlWindow::default().with_size(w, h);
        win.set_mode(Mode::Opengl3 | Mode::Double | Mode::Rgb8);
        win.end();
        let st = Rc::new(RefCell::new(State {
            source: fragment.to_string(),
            gl: None,
            error: None,
            fps: 0,
            started: Instant::now(),
            frozen: None,
            params: [0.0; 4],
            pointer: None,
            stats: GpuStats::default(),
            timer: false,
            readback: Vec::new(),
        }));
        let s2 = st.clone();
        win.draw(move |w| draw(w, &s2));
        // cfltk runs FLTK's own draw before this closure by default: it would
        // draw damaged children first and this background would then cover them.
        win.super_draw(false);
        let s3 = st.clone();
        win.handle(move |w, ev| match ev {
            Event::Enter | Event::Move => {
                s3.borrow_mut().pointer = Some((app::event_x() as f32, app::event_y() as f32));
                if s3.borrow().fps == 0 {
                    w.redraw();
                }
                true
            }
            Event::Leave => {
                s3.borrow_mut().pointer = None;
                false
            }
            _ => false,
        });
        let mut w2 = win.clone();
        theme::on_change_while(&win, move |_| w2.redraw());
        // FLTK's Wayland GL subwindow waits, on its first draw, for the
        // parent window to be on an output, which only happens once the
        // parent has shown a frame: drawing both in the same first flush
        // never returns. So the GL view stays hidden until its window has
        // been shown for a few frames.
        win.hide();
        let mut w3 = win.clone();
        let ticks = std::cell::Cell::new(0u32);
        app::add_timeout3(0.05, move |h| {
            if w3.was_deleted() {
                return;
            }
            let parent_up = w3.top_window().is_some_and(|t| t.shown() && t.visible());
            if parent_up {
                ticks.set(ticks.get() + 1);
            }
            if ticks.get() >= 3 {
                w3.show();
                // A Flex skipped the hidden view: lay it out again.
                if let Some(parent) = w3.parent() {
                    if let Some(f) = fltk::group::Flex::from_dyn_widget(&parent) {
                        f.layout();
                    }
                    parent.clone().redraw();
                }
                w3.redraw();
            } else {
                app::repeat_timeout3(0.05, h);
            }
        });
        GpuView { win, st }
    }

    /// True once the GL view has been shown (see the note in `new`).
    pub fn is_live(&self) -> bool {
        self.win.shown() && self.win.visible()
    }

    /// Redraws `fps` times per second (0 stops); ticks are skipped while hidden.
    pub fn animate(self, fps: u32) -> Self {
        self.st.borrow_mut().fps = fps.min(240);
        if !std::mem::replace(&mut self.st.borrow_mut().timer, true) && fps > 0 {
            let (st, w) = (Rc::downgrade(&self.st), self.win.clone());
            app::add_timeout3(1.0 / fps as f64, move |h| {
                let Some(st) = st.upgrade() else { return };
                if w.was_deleted() {
                    return;
                }
                let fps = st.borrow().fps;
                if fps == 0 {
                    st.borrow_mut().timer = false;
                    return;
                }
                if w.visible_r() && w.shown() {
                    w.clone().redraw();
                    app::repeat_timeout3(1.0 / fps as f64, h);
                } else {
                    st.borrow_mut().stats.paused_ticks += 1;
                    app::repeat_timeout3(0.5, h);
                }
            });
        }
        self
    }

    pub fn set_params(&self, p: [f32; 4]) {
        self.st.borrow_mut().params = p;
        self.win.clone().redraw();
    }

    /// Freezes `u_time` (screenshots, tests); None follows the clock.
    pub fn set_time(&self, t: Option<f32>) {
        self.st.borrow_mut().frozen = t;
        self.win.clone().redraw();
    }

    /// Replaces the fragment code (recompiled on the next draw).
    pub fn set_source(&self, fragment: &str) {
        let mut s = self.st.borrow_mut();
        s.source = fragment.to_string();
        if let Some(gl) = s.gl.take() {
            // SAFETY: deleting objects of this view's own context on the UI thread.
            unsafe {
                gl.ctx.delete_program(gl.program);
                gl.ctx.delete_vertex_array(gl.vao);
            }
        }
        drop(s);
        self.win.clone().redraw();
    }

    /// Shader compile or link error of the last draw, if any.
    pub fn last_error(&self) -> Option<String> {
        self.st.borrow().error.clone()
    }

    pub fn stats(&self) -> GpuStats {
        self.st.borrow().stats
    }

    /// Reads the pixel at widget coordinates `(x, y)` after the next draw
    /// (tests: the GPU output cannot be captured by offscreen snapshots).
    pub fn read_pixel(&self, x: i32, y: i32, f: impl FnOnce([u8; 4]) + 'static) {
        let scale = self.win.pixel_w() as f32 / self.win.w().max(1) as f32;
        let (px, py) = ((x as f32 * scale) as i32, ((self.win.h() - 1 - y) as f32 * scale) as i32);
        self.st.borrow_mut().readback.push((px, py, Box::new(f)));
        self.win.clone().redraw();
    }
}

fn compile(ctx: &glow::Context, fragment: &str) -> Result<(glow::Program, glow::VertexArray), String> {
    // SAFETY: called inside the window's draw with its context current.
    unsafe {
        let program = ctx.create_program()?;
        let mut shaders = Vec::new();
        for (kind, src) in [(glow::VERTEX_SHADER, VERTEX.to_string()), (glow::FRAGMENT_SHADER, format!("{PRELUDE}{fragment}"))] {
            let s = ctx.create_shader(kind)?;
            ctx.shader_source(s, &src);
            ctx.compile_shader(s);
            if !ctx.get_shader_compile_status(s) {
                let log = ctx.get_shader_info_log(s);
                ctx.delete_shader(s);
                ctx.delete_program(program);
                return Err(log);
            }
            ctx.attach_shader(program, s);
            shaders.push(s);
        }
        ctx.link_program(program);
        for s in shaders {
            ctx.detach_shader(program, s);
            ctx.delete_shader(s);
        }
        if !ctx.get_program_link_status(program) {
            let log = ctx.get_program_info_log(program);
            ctx.delete_program(program);
            return Err(log);
        }
        let vao = ctx.create_vertex_array()?;
        Ok((program, vao))
    }
}

fn draw(w: &mut GlWindow, st: &Rc<RefCell<State>>) {
    let t0 = Instant::now();
    let mut s = st.borrow_mut();
    let bg = theme::with(|t| t.palette.background);
    if s.gl.is_none() && s.error.is_none() {
        let win = w.clone();
        // SAFETY: the loader returns function pointers of the current context.
        let ctx = unsafe { glow::Context::from_loader_function(|name| win.get_proc_address(name) as *const _) };
        match compile(&ctx, &s.source) {
            Ok((program, vao)) => s.gl = Some(Gl { ctx, program, vao }),
            Err(e) => s.error = Some(e),
        }
    }
    let (pw, ph) = (w.pixel_w(), w.pixel_h());
    let time = s.frozen.unwrap_or_else(|| s.started.elapsed().as_secs_f32());
    let (params, pointer) = (s.params, s.pointer);
    let readback = std::mem::take(&mut s.readback);
    s.stats.pixel_size = (w.pixel_w(), w.pixel_h());
    let Some(gl) = s.gl.as_ref() else {
        // Compile error: theme background, nothing else.
        fltk::draw::set_draw_color(bg.fl());
        fltk::draw::draw_rectf(0, 0, w.w(), w.h());
        return;
    };
    let col = |c: theme::Rgb| [c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0, 1.0];
    let (fg, acc, urg) = theme::with(|t| (t.palette.foreground, t.palette.accent, t.palette.urgent));
    // SAFETY: inside the draw callback, the window's context is current.
    unsafe {
        let ctx = &gl.ctx;
        ctx.viewport(0, 0, pw, ph);
        ctx.use_program(Some(gl.program));
        let loc = |n: &str| ctx.get_uniform_location(gl.program, n);
        ctx.uniform_1_f32(loc("u_time").as_ref(), time);
        ctx.uniform_2_f32(loc("u_size").as_ref(), w.w() as f32, w.h() as f32);
        let (px, py) = pointer.unwrap_or((-1.0, -1.0));
        ctx.uniform_2_f32(loc("u_pointer").as_ref(), px, py);
        ctx.uniform_1_f32(loc("u_scale").as_ref(), pw as f32 / w.w().max(1) as f32);
        for (n, c) in [("u_background", col(bg)), ("u_foreground", col(fg)), ("u_accent", col(acc)), ("u_urgent", col(urg))] {
            ctx.uniform_4_f32(loc(n).as_ref(), c[0], c[1], c[2], c[3]);
        }
        ctx.uniform_4_f32(loc("u_params").as_ref(), params[0], params[1], params[2], params[3]);
        ctx.bind_vertex_array(Some(gl.vao));
        ctx.draw_arrays(glow::TRIANGLES, 0, 3);
        if !readback.is_empty() {
            let mut out = Vec::new();
            for (x, y, f) in readback {
                let mut px = [0u8; 4];
                ctx.read_pixels(x, y, 1, 1, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut px)));
                out.push((f, px));
            }
            s.stats.frames += 1;
            s.stats.last_draw_ms = t0.elapsed().as_secs_f32() * 1000.0;
            drop(s);
            for (f, px) in out {
                f(px);
            }
            return;
        }
    }
    s.stats.frames += 1;
    s.stats.last_draw_ms = t0.elapsed().as_secs_f32() * 1000.0;
}

// ------------------------------------------------------------ View's GL layer

const BLIT_VERTEX: &str = "#version 330 core
out vec2 v_uv;
void main() {
    vec2 v = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
    v_uv = vec2(v.x, 1.0 - v.y);
    gl_Position = vec4(v * 2.0 - 1.0, 0.0, 1.0);
}
";

const BLIT_FRAGMENT: &str = "#version 330 core
in vec2 v_uv;
uniform sampler2D u_tex;
out vec4 o_color;
void main() { o_color = texture(u_tex, v_uv); }
";

fn link(ctx: &glow::Context, vs: &str, fs: &str) -> Result<glow::Program, String> {
    // SAFETY: called inside the window's draw with its context current.
    unsafe {
        let program = ctx.create_program()?;
        let mut shaders = Vec::new();
        for (kind, src) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
            let s = ctx.create_shader(kind)?;
            ctx.shader_source(s, src);
            ctx.compile_shader(s);
            if !ctx.get_shader_compile_status(s) {
                let log = ctx.get_shader_info_log(s);
                ctx.delete_program(program);
                return Err(log);
            }
            ctx.attach_shader(program, s);
            shaders.push(s);
        }
        ctx.link_program(program);
        for s in shaders {
            ctx.detach_shader(program, s);
            ctx.delete_shader(s);
        }
        if !ctx.get_program_link_status(program) {
            return Err(ctx.get_program_info_log(program));
        }
        Ok(program)
    }
}

struct LayerGl {
    ctx: glow::Context,
    shade: glow::Program,
    blit: glow::Program,
    vao: glow::VertexArray,
    tex: glow::Texture,
}

/// What the View hands to its GL layer every frame.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LayerFrame {
    pub time: f32,
    pub params: [f32; 4],
    pub pointer: Option<(f32, f32)>,
}

pub(crate) struct LayerState {
    glsl: String,
    gl: Option<LayerGl>,
    pub error: Option<String>,
    pub frame: LayerFrame,
    /// Premultiplied RGBA overlay (paint + focus ring), widget size.
    overlay: Vec<u8>,
    overlay_size: (i32, i32),
    overlay_dirty: bool,
    pub frames: u64,
    pub last_draw_ms: f32,
    readback: Vec<(i32, i32, Box<dyn FnOnce([u8; 4])>)>,
}

/// The GL subwindow a [`super::View`] uses when it renders on the GPU.
#[derive(Clone)]
pub(crate) struct GlLayer {
    pub win: GlWindow,
    pub st: Rc<RefCell<LayerState>>,
}

impl GlLayer {
    /// A GL subwindow covering `(x, y, w, h)` of the current group, running
    /// `glsl` (from `sl::Shader::glsl`). `events(ev, x, y)` gets mouse
    /// events in local coordinates.
    pub(crate) fn new(x: i32, y: i32, w: i32, h: i32, glsl: &str, mut events: impl FnMut(Event, i32, i32) -> bool + 'static) -> GlLayer {
        let mut win = GlWindow::new(x, y, w, h, None);
        win.set_mode(Mode::Opengl3 | Mode::Double | Mode::Rgb8);
        win.end();
        let st = Rc::new(RefCell::new(LayerState {
            glsl: glsl.to_string(),
            gl: None,
            error: None,
            frame: LayerFrame::default(),
            overlay: Vec::new(),
            overlay_size: (0, 0),
            overlay_dirty: false,
            frames: 0,
            last_draw_ms: 0.0,
            readback: Vec::new(),
        }));
        let s2 = st.clone();
        win.draw(move |w| layer_draw(w, &s2));
        // cfltk runs FLTK's own draw before this closure by default: it would
        // draw damaged children first and this background would then cover them.
        win.super_draw(false);
        win.handle(move |_, ev| events(ev, app::event_x(), app::event_y()));
        GlLayer { win, st }
    }

    pub(crate) fn set_overlay(&self, rgba_premul: &[u8], w: i32, h: i32) {
        let mut s = self.st.borrow_mut();
        s.overlay.clear();
        s.overlay.extend_from_slice(rgba_premul);
        s.overlay_size = (w, h);
        s.overlay_dirty = true;
    }

    pub(crate) fn read_pixel(&self, x: i32, y: i32, f: Box<dyn FnOnce([u8; 4])>) {
        let scale = self.win.pixel_w() as f32 / self.win.w().max(1) as f32;
        let (px, py) = ((x as f32 * scale) as i32, ((self.win.h() - 1 - y) as f32 * scale) as i32);
        self.st.borrow_mut().readback.push((px, py, f));
    }
}

fn layer_draw(w: &mut GlWindow, st: &Rc<RefCell<LayerState>>) {
    let t0 = Instant::now();
    let mut s = st.borrow_mut();
    if s.gl.is_none() && s.error.is_none() {
        let win = w.clone();
        // Never block in eglSwapBuffers waiting for a frame callback (hidden
        // windows get none): pacing comes from the View's timer.
        w.set_swap_interval(0);
        // SAFETY: the loader returns function pointers of the current context.
        let ctx = unsafe { glow::Context::from_loader_function(|name| win.get_proc_address(name) as *const _) };
        let built = (|| -> Result<LayerGl, String> {
            let shade = link(&ctx, VERTEX, &format!("{PRELUDE}{}", s.glsl))?;
            let blit = link(&ctx, BLIT_VERTEX, BLIT_FRAGMENT)?;
            // SAFETY: context current (inside draw).
            unsafe {
                let vao = ctx.create_vertex_array()?;
                let tex = ctx.create_texture()?;
                ctx.bind_texture(glow::TEXTURE_2D, Some(tex));
                ctx.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
                ctx.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                ctx.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
                ctx.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
                Ok(LayerGl { ctx, shade, blit, vao, tex })
            }
        })();
        match built {
            Ok(g) => s.gl = Some(g),
            Err(e) => s.error = Some(e),
        }
    }
    let (pw, ph) = (w.pixel_w(), w.pixel_h());
    let frame = s.frame;
    let readback = std::mem::take(&mut s.readback);
    let overlay = if s.overlay_dirty && s.overlay_size.0 > 0 { Some((std::mem::take(&mut s.overlay), s.overlay_size)) } else { None };
    s.overlay_dirty = false;
    let has_overlay = s.overlay_size.0 > 0;
    let Some(gl) = s.gl.as_ref() else { return };
    let col = |c: theme::Rgb| [c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0, 1.0];
    let (bg, fg, acc, urg) = theme::with(|t| (t.palette.background, t.palette.foreground, t.palette.accent, t.palette.urgent));
    let mut out = Vec::new();
    // SAFETY: inside the draw callback, the window's context is current.
    unsafe {
        let ctx = &gl.ctx;
        ctx.viewport(0, 0, pw, ph);
        ctx.disable(glow::BLEND);
        ctx.use_program(Some(gl.shade));
        let loc = |n: &str| ctx.get_uniform_location(gl.shade, n);
        ctx.uniform_1_f32(loc("u_time").as_ref(), frame.time);
        ctx.uniform_2_f32(loc("u_size").as_ref(), w.w() as f32, w.h() as f32);
        let (px, py) = frame.pointer.unwrap_or((-1.0, -1.0));
        ctx.uniform_2_f32(loc("u_pointer").as_ref(), px, py);
        ctx.uniform_1_f32(loc("u_scale").as_ref(), pw as f32 / w.w().max(1) as f32);
        for (n, c) in [("u_background", col(bg)), ("u_foreground", col(fg)), ("u_accent", col(acc)), ("u_urgent", col(urg))] {
            ctx.uniform_4_f32(loc(n).as_ref(), c[0], c[1], c[2], c[3]);
        }
        let p = frame.params;
        ctx.uniform_4_f32(loc("u_params").as_ref(), p[0], p[1], p[2], p[3]);
        ctx.bind_vertex_array(Some(gl.vao));
        ctx.draw_arrays(glow::TRIANGLES, 0, 3);
        // Overlay (paint + focus ring), premultiplied alpha.
        ctx.bind_texture(glow::TEXTURE_2D, Some(gl.tex));
        if let Some((data, (ow, oh))) = &overlay {
            ctx.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            ctx.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, *ow, *oh, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(data)));
        }
        if has_overlay {
            ctx.enable(glow::BLEND);
            ctx.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            ctx.use_program(Some(gl.blit));
            ctx.uniform_1_i32(ctx.get_uniform_location(gl.blit, "u_tex").as_ref(), 0);
            ctx.draw_arrays(glow::TRIANGLES, 0, 3);
        }
        for (x, y, f) in readback {
            let mut px = [0u8; 4];
            ctx.read_pixels(x, y, 1, 1, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut px)));
            out.push((f, px));
        }
    }
    if let Some((data, _)) = overlay {
        s.overlay = data; // keep the allocation
    }
    s.frames += 1;
    s.last_draw_ms = t0.elapsed().as_secs_f32() * 1000.0;
    drop(s);
    for (f, px) in out {
        f(px);
    }
}
