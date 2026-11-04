//! Portable shaders: write the effect once in Rust, the toolkit runs it on
//! the GPU (generated GLSL) or the CPU (a row interpreter), whichever the
//! [`super::View`] picked.
//!
//! ```
//! use fltk_omarchy::gfx::sl::*;
//! let glow = Shader::new(|p, u| {
//!     let d = length(p - u.size() * 0.5) / min(u.size().x(), u.size().y());
//!     mix(u.accent(), u.background(), smoothstep(0.0, 0.5 + 0.05 * sin(u.time() * 2.0), d))
//! });
//! assert!(glow.glsl().contains("vec4 shade(vec2 p)"));
//! ```
//!
//! Values: [`F`] (float), [`V2`] (position, size) and [`V4`] (RGBA color),
//! with `+ - * /` (a float works with any of them) and the functions below.
//! `p` is the pixel center in widget pixels, y down. Uniforms from `u`:
//! `time()`, `size()`, `pointer()` (−1, −1 when outside), the theme colors
//! `background()`, `foreground()`, `accent()`, `urgent()`, and `param(0..4)`.
//! `noise`, `fbm` and `hash` use an integer hash, so both backends compute
//! the same pattern.

use super::canvas::Color;
use super::shader::{hash2, Uniforms};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    F,
    V2,
    V4,
}

impl Ty {
    fn dims(self) -> usize {
        match self {
            Ty::F => 1,
            Ty::V2 => 2,
            Ty::V4 => 4,
        }
    }
    fn glsl(self) -> &'static str {
        match self {
            Ty::F => "float",
            Ty::V2 => "vec2",
            Ty::V4 => "vec4",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Const([f32; 4]),
    Pos,
    Time,
    Size,
    Pointer,
    Bg,
    Fg,
    Accent,
    Urgent,
    Param(u8),
    Add,
    Sub,
    Mul,
    Div,
    Neg,
    Sin,
    Cos,
    Abs,
    Floor,
    Fract,
    Sqrt,
    Exp,
    Pow,
    Min,
    Max,
    Clamp,
    Mix,
    Step,
    Smoothstep,
    Length,
    Dot,
    Atan2,
    Vec2,
    Vec4,
    Comp(u8),
    WithAlpha,
    Hash,
    Noise,
    Fbm(u8),
}

#[doc(hidden)]
#[derive(Debug)]
pub struct Node {
    op: Op,
    ty: Ty,
    args: Vec<Rc<Node>>,
}

fn node(op: Op, ty: Ty, args: Vec<Rc<Node>>) -> Rc<Node> {
    Rc::new(Node { op, ty, args })
}

/// A float expression.
#[derive(Debug, Clone)]
pub struct F(Rc<Node>);
/// A 2-component expression (positions, sizes).
#[derive(Debug, Clone)]
pub struct V2(Rc<Node>);
/// A 4-component expression (RGBA colors, straight alpha).
#[derive(Debug, Clone)]
pub struct V4(Rc<Node>);

/// Common to [`F`], [`V2`] and [`V4`]: component-wise functions accept any.
pub trait Value: Clone {
    #[doc(hidden)]
    fn n(&self) -> Rc<Node>;
    #[doc(hidden)]
    fn wrap(n: Rc<Node>) -> Self;
    #[doc(hidden)]
    const TY: Ty;
}

impl Value for F {
    fn n(&self) -> Rc<Node> { self.0.clone() }
    fn wrap(n: Rc<Node>) -> Self { F(n) }
    const TY: Ty = Ty::F;
}
impl Value for V2 {
    fn n(&self) -> Rc<Node> { self.0.clone() }
    fn wrap(n: Rc<Node>) -> Self { V2(n) }
    const TY: Ty = Ty::V2;
}
impl Value for V4 {
    fn n(&self) -> Rc<Node> { self.0.clone() }
    fn wrap(n: Rc<Node>) -> Self { V4(n) }
    const TY: Ty = Ty::V4;
}

impl From<f32> for F {
    fn from(v: f32) -> F {
        F(node(Op::Const([v, 0.0, 0.0, 0.0]), Ty::F, vec![]))
    }
}

impl From<Color> for V4 {
    fn from(c: Color) -> V4 {
        V4(node(Op::Const([c.r, c.g, c.b, c.a]), Ty::V4, vec![]))
    }
}

/// A float constant.
pub fn lit(v: f32) -> F {
    F::from(v)
}

pub fn vec2(x: impl Into<F>, y: impl Into<F>) -> V2 {
    V2(node(Op::Vec2, Ty::V2, vec![x.into().0, y.into().0]))
}

pub fn rgba(r: impl Into<F>, g: impl Into<F>, b: impl Into<F>, a: impl Into<F>) -> V4 {
    V4(node(Op::Vec4, Ty::V4, vec![r.into().0, g.into().0, b.into().0, a.into().0]))
}

impl V2 {
    pub fn x(&self) -> F { F(node(Op::Comp(0), Ty::F, vec![self.0.clone()])) }
    pub fn y(&self) -> F { F(node(Op::Comp(1), Ty::F, vec![self.0.clone()])) }
}

impl V4 {
    pub fn r(&self) -> F { F(node(Op::Comp(0), Ty::F, vec![self.0.clone()])) }
    pub fn g(&self) -> F { F(node(Op::Comp(1), Ty::F, vec![self.0.clone()])) }
    pub fn b(&self) -> F { F(node(Op::Comp(2), Ty::F, vec![self.0.clone()])) }
    pub fn a(&self) -> F { F(node(Op::Comp(3), Ty::F, vec![self.0.clone()])) }
    pub fn with_alpha(&self, a: impl Into<F>) -> V4 {
        V4(node(Op::WithAlpha, Ty::V4, vec![self.0.clone(), a.into().0]))
    }
}

// ---- operators: same type, or a float with anything

macro_rules! binop {
    ($tr:ident, $f:ident, $op:expr) => {
        impl<T: Value> std::ops::$tr<T> for F where T: NotF {
            type Output = T;
            fn $f(self, b: T) -> T { T::wrap(node($op, T::TY, vec![self.0, b.n()])) }
        }
        impl std::ops::$tr<F> for F {
            type Output = F;
            fn $f(self, b: F) -> F { F(node($op, Ty::F, vec![self.0, b.0])) }
        }
        impl std::ops::$tr<f32> for F {
            type Output = F;
            fn $f(self, b: f32) -> F { F(node($op, Ty::F, vec![self.0, lit(b).0])) }
        }
        impl std::ops::$tr<F> for f32 {
            type Output = F;
            fn $f(self, b: F) -> F { F(node($op, Ty::F, vec![lit(self).0, b.0])) }
        }
        binop!(@vec V2, $tr, $f, $op);
        binop!(@vec V4, $tr, $f, $op);
    };
    (@vec $V:ident, $tr:ident, $f:ident, $op:expr) => {
        impl std::ops::$tr<$V> for $V {
            type Output = $V;
            fn $f(self, b: $V) -> $V { $V(node($op, $V::TY, vec![self.0, b.0])) }
        }
        impl std::ops::$tr<F> for $V {
            type Output = $V;
            fn $f(self, b: F) -> $V { $V(node($op, $V::TY, vec![self.0, b.0])) }
        }
        impl std::ops::$tr<f32> for $V {
            type Output = $V;
            fn $f(self, b: f32) -> $V { $V(node($op, $V::TY, vec![self.0, lit(b).0])) }
        }
        impl std::ops::$tr<$V> for f32 {
            type Output = $V;
            fn $f(self, b: $V) -> $V { $V(node($op, $V::TY, vec![lit(self).0, b.0])) }
        }
    };
}

/// Marker: vector types (a float on the left of a vector gives the vector).
pub trait NotF: Value {}
impl NotF for V2 {}
impl NotF for V4 {}

binop!(Add, add, Op::Add);
binop!(Sub, sub, Op::Sub);
binop!(Mul, mul, Op::Mul);
binop!(Div, div, Op::Div);

macro_rules! neg {
    ($($T:ident),*) => {$(
        impl std::ops::Neg for $T {
            type Output = $T;
            fn neg(self) -> $T { $T(node(Op::Neg, $T::TY, vec![self.0])) }
        }
    )*};
}
neg!(F, V2, V4);

// ---- functions

macro_rules! unary {
    ($($name:ident => $op:expr),*) => {$(
        pub fn $name<T: Value>(x: T) -> T { T::wrap(node($op, T::TY, vec![x.n()])) }
    )*};
}
unary!(sin => Op::Sin, cos => Op::Cos, abs => Op::Abs, floor => Op::Floor, fract => Op::Fract, sqrt => Op::Sqrt, exp => Op::Exp);

pub fn pow<T: Value>(x: T, e: impl Into<F>) -> T { T::wrap(node(Op::Pow, T::TY, vec![x.n(), e.into().0])) }
pub fn min<T: Value>(a: T, b: T) -> T { T::wrap(node(Op::Min, T::TY, vec![a.n(), b.n()])) }
pub fn max<T: Value>(a: T, b: T) -> T { T::wrap(node(Op::Max, T::TY, vec![a.n(), b.n()])) }
pub fn clamp<T: Value>(x: T, lo: impl Into<F>, hi: impl Into<F>) -> T { T::wrap(node(Op::Clamp, T::TY, vec![x.n(), lo.into().0, hi.into().0])) }
/// `a + (b - a) * t`.
pub fn mix<T: Value>(a: T, b: T, t: impl Into<F>) -> T { T::wrap(node(Op::Mix, T::TY, vec![a.n(), b.n(), t.into().0])) }
/// 0 below `edge`, 1 from it.
pub fn step(edge: impl Into<F>, x: impl Into<F>) -> F { F(node(Op::Step, Ty::F, vec![edge.into().0, x.into().0])) }
pub fn smoothstep(e0: impl Into<F>, e1: impl Into<F>, x: impl Into<F>) -> F {
    F(node(Op::Smoothstep, Ty::F, vec![e0.into().0, e1.into().0, x.into().0]))
}
pub fn length(v: V2) -> F { F(node(Op::Length, Ty::F, vec![v.0])) }
pub fn distance(a: V2, b: V2) -> F { length(a - b) }
pub fn dot(a: V2, b: V2) -> F { F(node(Op::Dot, Ty::F, vec![a.0, b.0])) }
pub fn atan2(y: impl Into<F>, x: impl Into<F>) -> F { F(node(Op::Atan2, Ty::F, vec![y.into().0, x.into().0])) }
/// Hash of the integer cell of `p`, 0..1.
pub fn hash(p: V2) -> F { F(node(Op::Hash, Ty::F, vec![p.0])) }
/// Smooth value noise, about 0..1 (one unit = one cell).
pub fn noise(p: V2) -> F { F(node(Op::Noise, Ty::F, vec![p.0])) }
/// Fractal noise: `octaves` (1..8) layers of `noise`, about 0..1.
pub fn fbm(p: V2, octaves: u8) -> F { F(node(Op::Fbm(octaves.clamp(1, 8)), Ty::F, vec![p.0])) }

/// The uniforms a shader can read.
pub struct U(());

impl U {
    pub fn time(&self) -> F { F(node(Op::Time, Ty::F, vec![])) }
    pub fn size(&self) -> V2 { V2(node(Op::Size, Ty::V2, vec![])) }
    pub fn pointer(&self) -> V2 { V2(node(Op::Pointer, Ty::V2, vec![])) }
    pub fn background(&self) -> V4 { V4(node(Op::Bg, Ty::V4, vec![])) }
    pub fn foreground(&self) -> V4 { V4(node(Op::Fg, Ty::V4, vec![])) }
    pub fn accent(&self) -> V4 { V4(node(Op::Accent, Ty::V4, vec![])) }
    pub fn urgent(&self) -> V4 { V4(node(Op::Urgent, Ty::V4, vec![])) }
    /// Free parameter 0..3 (`View::set_params`).
    pub fn param(&self, i: u8) -> F { F(node(Op::Param(i.min(3)), Ty::F, vec![])) }
}

// ---------------------------------------------------------------- compile

#[derive(Debug, Clone)]
struct Instr {
    op: Op,
    ty: Ty,
    args: Vec<usize>,
    /// Does not depend on the pixel: computed once per frame.
    uniform: bool,
}

/// A compiled portable shader (thread-safe; cheap to clone).
#[derive(Debug, Clone)]
pub struct Shader {
    prog: Arc<Vec<Instr>>,
    glsl: Arc<String>,
}

impl Shader {
    /// Builds the shader once from `f(p, u)` → color.
    pub fn new(f: impl FnOnce(V2, &U) -> V4) -> Shader {
        let p = V2(node(Op::Pos, Ty::V2, vec![]));
        let out = f(p, &U(()));
        let mut prog: Vec<Instr> = Vec::new();
        let mut index: HashMap<*const Node, usize> = HashMap::new();
        fn visit(n: &Rc<Node>, prog: &mut Vec<Instr>, index: &mut HashMap<*const Node, usize>) -> usize {
            if let Some(&i) = index.get(&Rc::as_ptr(n)) {
                return i;
            }
            let args: Vec<usize> = n.args.iter().map(|a| visit(a, prog, index)).collect();
            let uniform = n.op != Op::Pos && args.iter().all(|&a| prog[a].uniform);
            prog.push(Instr { op: n.op, ty: n.ty, args, uniform });
            index.insert(Rc::as_ptr(n), prog.len() - 1);
            prog.len() - 1
        }
        visit(&out.0, &mut prog, &mut index);
        let glsl = codegen(&prog);
        Shader { prog: Arc::new(prog), glsl: Arc::new(glsl) }
    }

    /// GLSL for the GPU backend: defines `vec4 shade(vec2 p)` and the
    /// `sl_hash`/`sl_noise`/`sl_fbm` helpers.
    pub fn glsl(&self) -> &str {
        &self.glsl
    }

    /// Number of operations (cost indicator).
    pub fn ops(&self) -> usize {
        self.prog.len()
    }

    /// Evaluates one pixel on the CPU (tests, tools). Rendering uses rows.
    pub fn eval(&self, x: f32, y: f32, u: &Uniforms) -> Color {
        let mut e = Exec::default();
        let mut out = [Color::TRANSPARENT];
        e.row(self, x - 0.5, y - 0.5, 1, (1.0, 1.0), u, &mut out);
        out[0]
    }
}

fn fmt_f(v: f32) -> String {
    let s = format!("{v:?}");
    if s.contains('.') || s.contains('e') || s.contains("inf") || s.contains("NaN") { s } else { format!("{s}.0") }
}

const GLSL_HELPERS: &str = "float sl_hash(vec2 p, uint seed) {
    uvec2 q = uvec2(ivec2(floor(p)));
    uint h = (q.x * 0x8da6b343u) ^ (q.y * 0xd8163841u) ^ (seed * 0xcb1ab31fu);
    h = (h ^ (h >> 13u)) * 0x5bd1e995u;
    h ^= h >> 15u;
    return float(h & 0x00ffffffu) / 16777215.0;
}
float sl_noise(vec2 p, uint seed) {
    vec2 i = floor(p);
    vec2 f = p - i;
    vec2 u = f * f * (3.0 - 2.0 * f);
    float a = sl_hash(i, seed), b = sl_hash(i + vec2(1.0, 0.0), seed);
    float c = sl_hash(i + vec2(0.0, 1.0), seed), d = sl_hash(i + vec2(1.0, 1.0), seed);
    return a + (b - a) * u.x + (c - a) * u.y + (a - b - c + d) * u.x * u.y;
}
float sl_fbm(vec2 p, int octaves) {
    float sum = 0.0, amp = 0.5, freq = 1.0, norm = 0.0;
    for (int o = 0; o < octaves; o++) {
        sum += amp * sl_noise(p * freq, 7u + uint(o) * 101u);
        norm += amp; amp *= 0.5; freq *= 2.0;
    }
    return sum / norm;
}
";

fn codegen(prog: &[Instr]) -> String {
    let mut s = String::from(GLSL_HELPERS);
    s.push_str("vec4 shade(vec2 p) {\n");
    for (i, ins) in prog.iter().enumerate() {
        let a = |k: usize| format!("t{}", ins.args[k]);
        let e = match ins.op {
            Op::Const(c) => match ins.ty {
                Ty::F => fmt_f(c[0]),
                Ty::V2 => format!("vec2({}, {})", fmt_f(c[0]), fmt_f(c[1])),
                Ty::V4 => format!("vec4({}, {}, {}, {})", fmt_f(c[0]), fmt_f(c[1]), fmt_f(c[2]), fmt_f(c[3])),
            },
            Op::Pos => "p".into(),
            Op::Time => "u_time".into(),
            Op::Size => "u_size".into(),
            Op::Pointer => "u_pointer".into(),
            Op::Bg => "u_background".into(),
            Op::Fg => "u_foreground".into(),
            Op::Accent => "u_accent".into(),
            Op::Urgent => "u_urgent".into(),
            Op::Param(k) => format!("u_params[{k}]"),
            Op::Add => format!("{} + {}", a(0), a(1)),
            Op::Sub => format!("{} - {}", a(0), a(1)),
            Op::Mul => format!("{} * {}", a(0), a(1)),
            Op::Div => format!("{} / {}", a(0), a(1)),
            Op::Neg => format!("-{}", a(0)),
            Op::Sin => format!("sin({})", a(0)),
            Op::Cos => format!("cos({})", a(0)),
            Op::Abs => format!("abs({})", a(0)),
            Op::Floor => format!("floor({})", a(0)),
            Op::Fract => format!("fract({})", a(0)),
            Op::Sqrt => format!("sqrt({})", a(0)),
            Op::Exp => format!("exp({})", a(0)),
            Op::Pow => format!("pow({}, {}({}))", a(0), ins.ty.glsl(), a(1)),
            Op::Min => format!("min({}, {})", a(0), a(1)),
            Op::Max => format!("max({}, {})", a(0), a(1)),
            Op::Clamp => format!("clamp({}, {}, {})", a(0), a(1), a(2)),
            Op::Mix => format!("mix({}, {}, {})", a(0), a(1), a(2)),
            Op::Step => format!("step({}, {})", a(0), a(1)),
            Op::Smoothstep => format!("smoothstep({}, {}, {})", a(0), a(1), a(2)),
            Op::Length => format!("length({})", a(0)),
            Op::Dot => format!("dot({}, {})", a(0), a(1)),
            Op::Atan2 => format!("atan({}, {})", a(0), a(1)),
            Op::Vec2 => format!("vec2({}, {})", a(0), a(1)),
            Op::Vec4 => format!("vec4({}, {}, {}, {})", a(0), a(1), a(2), a(3)),
            Op::Comp(k) => format!("{}.{}", a(0), ["x", "y", "z", "w"][k as usize]),
            Op::WithAlpha => format!("vec4({}.rgb, {})", a(0), a(1)),
            Op::Hash => format!("sl_hash({}, 7u)", a(0)),
            Op::Noise => format!("sl_noise({}, 7u)", a(0)),
            Op::Fbm(o) => format!("sl_fbm({}, {o})", a(0)),
        };
        s.push_str(&format!("    {} t{i} = {e};\n", ins.ty.glsl()));
    }
    s.push_str(&format!("    return t{};\n}}\n", prog.len() - 1));
    s
}

// ---------------------------------------------------------------- CPU

fn noise_cpu(x: f32, y: f32, seed: u32) -> f32 {
    let (fx, fy) = (x.floor(), y.floor());
    let (xi, yi) = (fx as i32, fy as i32);
    let (xf, yf) = (x - fx, y - fy);
    let (u, v) = (xf * xf * (3.0 - 2.0 * xf), yf * yf * (3.0 - 2.0 * yf));
    let a = hash2(xi, yi, seed);
    let b = hash2(xi + 1, yi, seed);
    let c = hash2(xi, yi + 1, seed);
    let d = hash2(xi + 1, yi + 1, seed);
    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

fn fbm_cpu(x: f32, y: f32, octaves: u8) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 0.5, 1.0, 0.0);
    for o in 0..octaves as u32 {
        sum += amp * noise_cpu(x * freq, y * freq, 7 + o * 101);
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

/// Scratch registers of the row interpreter (one per thread; reused).
#[derive(Default)]
pub(crate) struct Exec {
    regs: Vec<[f32; 4]>,
    lanes: usize,
}

impl Exec {
    /// Evaluates `n` pixels of row `y` starting at column `x0` (render
    /// pixels; `scale` maps them to widget pixels) into `out`.
    pub(crate) fn row(&mut self, sh: &Shader, x0: f32, y: f32, n: usize, scale: (f32, f32), u: &Uniforms, out: &mut [Color]) {
        let prog = &sh.prog;
        if self.lanes != n || self.regs.len() != prog.len() * n {
            self.lanes = n;
            self.regs = vec![[0.0; 4]; prog.len() * n];
        }
        let c4 = |c: Color| [c.r, c.g, c.b, c.a];
        for (i, ins) in prog.iter().enumerate() {
            let d = ins.ty.dims();
            // Uniform instructions: lane 0 only (read with stride 0).
            let lanes = if ins.uniform { 1 } else { n };
            for l in 0..lanes {
                let arg = |k: usize| -> [f32; 4] {
                    let a = ins.args[k];
                    let src = self.regs[a * n + if prog[a].uniform { 0 } else { l }];
                    if prog[a].ty == Ty::F { [src[0]; 4] } else { src }
                };
                let cw = |f: &dyn Fn(f32) -> f32, v: [f32; 4]| [f(v[0]), f(v[1]), f(v[2]), f(v[3])];
                let cw2 = |f: &dyn Fn(f32, f32) -> f32, a: [f32; 4], b: [f32; 4]| [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2]), f(a[3], b[3])];
                let v: [f32; 4] = match ins.op {
                    Op::Const(c) => c,
                    Op::Pos => [(x0 + l as f32 + 0.5) * scale.0, (y + 0.5) * scale.1, 0.0, 0.0],
                    Op::Time => [u.time; 4],
                    Op::Size => [u.size.0, u.size.1, 0.0, 0.0],
                    Op::Pointer => match u.pointer { Some((px, py)) => [px, py, 0.0, 0.0], None => [-1.0, -1.0, 0.0, 0.0] },
                    Op::Bg => c4(u.background),
                    Op::Fg => c4(u.foreground),
                    Op::Accent => c4(u.accent),
                    Op::Urgent => c4(u.urgent),
                    Op::Param(k) => [u.params[k as usize]; 4],
                    Op::Add => cw2(&|a, b| a + b, arg(0), arg(1)),
                    Op::Sub => cw2(&|a, b| a - b, arg(0), arg(1)),
                    Op::Mul => cw2(&|a, b| a * b, arg(0), arg(1)),
                    Op::Div => cw2(&|a, b| a / b, arg(0), arg(1)),
                    Op::Neg => cw(&|a| -a, arg(0)),
                    Op::Sin => cw(&f32::sin, arg(0)),
                    Op::Cos => cw(&f32::cos, arg(0)),
                    Op::Abs => cw(&f32::abs, arg(0)),
                    Op::Floor => cw(&f32::floor, arg(0)),
                    Op::Fract => cw(&|a| a - a.floor(), arg(0)),
                    Op::Sqrt => cw(&f32::sqrt, arg(0)),
                    Op::Exp => cw(&f32::exp, arg(0)),
                    Op::Pow => cw2(&f32::powf, arg(0), arg(1)),
                    Op::Min => cw2(&f32::min, arg(0), arg(1)),
                    Op::Max => cw2(&f32::max, arg(0), arg(1)),
                    Op::Clamp => {
                        let (x, lo, hi) = (arg(0), arg(1), arg(2));
                        [0, 1, 2, 3].map(|j| x[j].max(lo[j]).min(hi[j]))
                    }
                    Op::Mix => {
                        let (a, b, t) = (arg(0), arg(1), arg(2));
                        [0, 1, 2, 3].map(|j| a[j] + (b[j] - a[j]) * t[j])
                    }
                    Op::Step => {
                        let (e, x) = (arg(0), arg(1));
                        [if x[0] < e[0] { 0.0 } else { 1.0 }; 4]
                    }
                    Op::Smoothstep => {
                        let (e0, e1, x) = (arg(0)[0], arg(1)[0], arg(2)[0]);
                        let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
                        [t * t * (3.0 - 2.0 * t); 4]
                    }
                    Op::Length => {
                        let a = arg(0);
                        [(a[0] * a[0] + a[1] * a[1]).sqrt(); 4]
                    }
                    Op::Dot => {
                        let (a, b) = (arg(0), arg(1));
                        [a[0] * b[0] + a[1] * b[1]; 4]
                    }
                    Op::Atan2 => [arg(0)[0].atan2(arg(1)[0]); 4],
                    Op::Vec2 => [arg(0)[0], arg(1)[0], 0.0, 0.0],
                    Op::Vec4 => [arg(0)[0], arg(1)[0], arg(2)[0], arg(3)[0]],
                    Op::Comp(k) => [arg(0)[k as usize]; 4],
                    Op::WithAlpha => {
                        let (c, a) = (arg(0), arg(1));
                        [c[0], c[1], c[2], a[0]]
                    }
                    Op::Hash => {
                        let a = arg(0);
                        [hash2(a[0].floor() as i32, a[1].floor() as i32, 7); 4]
                    }
                    Op::Noise => {
                        let a = arg(0);
                        [noise_cpu(a[0], a[1], 7); 4]
                    }
                    Op::Fbm(o) => {
                        let a = arg(0);
                        [fbm_cpu(a[0], a[1], o); 4]
                    }
                };
                let _ = d;
                self.regs[i * n + l] = v;
            }
        }
        let last = prog.len() - 1;
        let uni = prog[last].uniform;
        for (l, o) in out.iter_mut().enumerate().take(n) {
            let v = self.regs[last * n + if uni { 0 } else { l }];
            *o = Color { r: v[0], g: v[1], b: v[2], a: v[3] };
        }
    }
}

// ---------------------------------------------------------------- built-ins

/// Animated soft noise between the theme background and foreground
/// (`param(0)` = scale in pixels, default 48).
pub fn noise_shader() -> Shader {
    Shader::new(|p, u| {
        let scale = max(u.param(0), lit(0.0)) + step(u.param(0), lit(0.0)) * 48.0;
        let q = p / scale + vec2(u.time() * 0.15, -u.time() * 0.1);
        mix(u.background(), u.foreground(), fbm(q, 3) * 0.35)
    })
}

/// Diagonal gradient from the background to the accent that slowly drifts.
pub fn gradient_shader() -> Shader {
    Shader::new(|p, u| {
        let s = u.size();
        let t = clamp(p.x() / s.x() * 0.7 + p.y() / s.y() * 0.3 + sin(u.time() * 0.6) * 0.25, 0.0, 1.0);
        let mid = mix(u.background(), u.accent(), 0.55);
        let top = mix(u.accent(), u.foreground(), 0.2);
        let lo = mix(u.background(), mid.clone(), t.clone() * 2.0);
        let hi = mix(mid, top, (t.clone() - 0.5) * 2.0);
        mix(lo, hi, step(0.5, t))
    })
}

/// Accent glow that breathes at the center, or follows the pointer.
pub fn accent_glow_shader() -> Shader {
    Shader::new(|p, u| {
        let center = u.size() * 0.5;
        let inside = step(0.0, u.pointer().x());
        let c = mix(center, u.pointer(), inside);
        let r = min(u.size().x(), u.size().y()) * (sin(u.time() * 2.0) * 0.04 + 0.32);
        let d = length(p - c) / max(r, lit(1.0));
        mix(u.background(), u.accent(), exp(-(d.clone() * d) * 2.2) * 0.85)
    })
}

/// Moving blobs (accent, foreground, urgent) to put behind a frosted panel.
pub fn blobs_shader() -> Shader {
    Shader::new(|p, u| {
        let (w, h) = (u.size().x(), u.size().y());
        let r = min(w.clone(), h.clone()) * 0.22;
        let mut c = u.background();
        let tints = [u.accent(), u.foreground(), u.urgent()];
        for (i, tint) in tints.into_iter().enumerate() {
            let fi = i as f32;
            let bx = w.clone() * (sin(u.time() * (0.5 + 0.2 * fi) + fi * 2.1) * 0.35 + 0.5);
            let by = h.clone() * (cos(u.time() * (0.4 + 0.15 * fi) + fi * 1.3) * 0.35 + 0.5);
            let d = length(p.clone() - vec2(bx, by)) / r.clone();
            c = mix(c, tint, clamp(1.0 - d, 0.0, 1.0) * 0.8);
        }
        c
    })
}

/// Concentric rings moving outward in the accent color.
pub fn rings_shader() -> Shader {
    Shader::new(|p, u| {
        let d = length(p - u.size() * 0.5);
        mix(u.background(), u.accent(), (sin(d * 0.15 - u.time() * 3.0) * 0.5 + 0.5) * 0.6)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uni() -> Uniforms {
        Uniforms {
            time: 1.5,
            frame: 0,
            size: (200.0, 100.0),
            pointer: None,
            background: Color::rgba(0.1, 0.1, 0.1, 1.0),
            foreground: Color::rgba(0.9, 0.9, 0.9, 1.0),
            accent: Color::rgba(0.9, 0.55, 0.05, 1.0),
            urgent: Color::rgba(0.8, 0.2, 0.2, 1.0),
            params: [0.0; 4],
        }
    }

    #[test]
    fn evaluates_like_the_formula() {
        let u = uni();
        let s = Shader::new(|p, u| mix(u.background(), u.accent(), clamp(p.x() / u.size().x(), 0.0, 1.0)));
        let c = s.eval(100.0, 10.0, &u);
        assert!((c.r - 0.5).abs() < 0.01 && (c.g - 0.325).abs() < 0.01, "{c:?}");
        let glow = accent_glow_shader();
        let center = glow.eval(100.0, 50.0, &u);
        let corner = glow.eval(1.0, 1.0, &u);
        assert!(center.r > corner.r, "glow brighter at the center");
        // Same hash as gfx::shaders::hash2: CPU and GPU agree on the pattern.
        let h = Shader::new(|p, _| { let v = hash(p); rgba(v.clone(), v.clone(), v, 1.0) });
        assert_eq!(h.eval(3.5, 7.5, &u).r, hash2(3, 7, 7));
    }

    #[test]
    fn rows_match_single_pixels_and_uniforms_are_hoisted() {
        let u = uni();
        let s = noise_shader();
        let mut e = Exec::default();
        let mut out = vec![Color::TRANSPARENT; 64];
        e.row(&s, 0.0, 12.0, 64, (1.0, 1.0), &u, &mut out);
        for x in [0usize, 17, 63] {
            assert_eq!(out[x], s.eval(x as f32 + 0.5, 12.5, &u));
        }
        assert!(s.prog.iter().filter(|i| i.uniform).count() > 3, "time/size math is per frame");
    }

    #[test]
    fn glsl_is_generated() {
        let g = accent_glow_shader();
        let src = g.glsl();
        assert!(src.contains("vec4 shade(vec2 p)") && src.contains("u_pointer") && src.contains("exp("));
        assert!(noise_shader().glsl().contains("sl_fbm(") && rings_shader().ops() > 5);
    }
}
