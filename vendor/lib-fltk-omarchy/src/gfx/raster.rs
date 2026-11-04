//! Anti-aliased coverage rasterizer (signed-area accumulation, the method
//! of font-rs) and stroke-to-polygon conversion.

use super::path::{dist, Point};

/// Accumulates signed area per pixel for a box of `w × h`; `finish` turns
/// it into 0..1 coverage (non-zero winding, union of overlapping parts).
pub(crate) struct Coverage {
    pub w: usize,
    pub h: usize,
    stride: usize,
    acc: Vec<f32>,
}

impl Coverage {
    pub fn new() -> Coverage {
        Coverage { w: 0, h: 0, stride: 0, acc: Vec::new() }
    }

    /// Resets for a box of `w × h`, reusing the allocation.
    pub fn reset(&mut self, w: usize, h: usize) {
        self.w = w;
        self.h = h;
        // Two spill columns: x is clamped to [0, w + 1).
        self.stride = w + 3;
        self.acc.clear();
        self.acc.resize(self.stride * h, 0.0);
    }

    pub fn polygon(&mut self, pts: &[Point], dx: f32, dy: f32) {
        let n = pts.len();
        if n < 2 {
            return;
        }
        for i in 0..n {
            let a = pts[i];
            let b = pts[(i + 1) % n];
            self.line((a.0 + dx, a.1 + dy), (b.0 + dx, b.1 + dy));
        }
    }

    fn line(&mut self, p0: Point, p1: Point) {
        if (p0.1 - p1.1).abs() <= f32::EPSILON || self.h == 0 {
            return;
        }
        let (dir, p0, p1) = if p0.1 < p1.1 { (1.0, p0, p1) } else { (-1.0, p1, p0) };
        if p1.1 <= 0.0 || p0.1 >= self.h as f32 {
            return;
        }
        let maxx = self.w as f32 + 1.0;
        let dxdy = (p1.0 - p0.0) / (p1.1 - p0.1);
        let y_start = p0.1.max(0.0);
        let mut x = p0.0 + (y_start - p0.1) * dxdy;
        let y0 = y_start as usize;
        let y1 = (p1.1.ceil() as usize).min(self.h);
        for y in y0..y1 {
            let row = y * self.stride;
            let dy = ((y + 1) as f32).min(p1.1) - (y as f32).max(p0.1);
            let xnext = x + dxdy * dy;
            let d = dy * dir;
            let (xa, xb) = if x < xnext { (x, xnext) } else { (xnext, x) };
            let (xa, xb) = (xa.clamp(0.0, maxx), xb.clamp(0.0, maxx));
            let x0f = xa.floor();
            let x0i = x0f as usize;
            let x1c = xb.ceil();
            let x1i = x1c as usize;
            if x1i <= x0i + 1 {
                let xm = 0.5 * (xa + xb) - x0f;
                self.acc[row + x0i] += d - d * xm;
                self.acc[row + x0i + 1] += d * xm;
            } else {
                let s = (xb - xa).recip();
                let x0r = xa - x0f;
                let a0 = 0.5 * s * (1.0 - x0r) * (1.0 - x0r);
                let x1r = xb - x1c + 1.0;
                let am = 0.5 * s * x1r * x1r;
                self.acc[row + x0i] += d * a0;
                if x1i == x0i + 2 {
                    self.acc[row + x0i + 1] += d * (1.0 - a0 - am);
                } else {
                    let a1 = s * (1.5 - x0r);
                    self.acc[row + x0i + 1] += d * (a1 - a0);
                    for xi in x0i + 2..x1i - 1 {
                        self.acc[row + xi] += d * s;
                    }
                    let a2 = a1 + (x1i - x0i - 3) as f32 * s;
                    self.acc[row + x1i - 1] += d * (1.0 - a2 - am);
                }
                self.acc[row + x1i] += d * am;
            }
            x = xnext;
        }
    }

    /// Calls `f(x, y, coverage)` for every pixel with coverage > 0.
    pub fn for_each(&self, mut f: impl FnMut(usize, usize, f32)) {
        for y in 0..self.h {
            let row = &self.acc[y * self.stride..y * self.stride + self.w];
            let mut sum = 0.0f32;
            for (x, a) in row.iter().enumerate() {
                sum += a;
                let c = sum.abs().min(1.0);
                if c > 1.0 / 512.0 {
                    f(x, y, c);
                }
            }
        }
    }
}

/// How stroke segments meet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Join {
    /// Sharp corner, beveled past `limit` × half the width.
    Miter(f32),
    Round,
    Bevel,
}

/// How open stroke ends look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cap {
    Butt,
    Round,
    Square,
}

/// Stroke style: `Stroke::new(2.0).join(Join::Round).cap(Cap::Round)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stroke {
    pub width: f32,
    pub join: Join,
    pub cap: Cap,
}

impl Stroke {
    pub fn new(width: f32) -> Stroke {
        Stroke { width, join: Join::Miter(4.0), cap: Cap::Butt }
    }
    pub fn join(mut self, j: Join) -> Self {
        self.join = j;
        self
    }
    pub fn cap(mut self, c: Cap) -> Self {
        self.cap = c;
        self
    }
}

fn area(p: &[Point]) -> f32 {
    let n = p.len();
    (0..n).map(|i| p[i].0 * p[(i + 1) % n].1 - p[(i + 1) % n].0 * p[i].1).sum::<f32>() / 2.0
}

/// Same orientation for every piece, so overlaps add up instead of cancel.
fn oriented(mut p: Vec<Point>) -> Vec<Point> {
    if area(&p) < 0.0 {
        p.reverse();
    }
    p
}

fn disc(c: Point, r: f32) -> Vec<Point> {
    let n = ((r * 2.0).ceil() as usize).clamp(8, 48);
    (0..n).map(|i| {
        let t = i as f32 / n as f32 * std::f32::consts::TAU;
        (c.0 + r * t.cos(), c.1 + r * t.sin())
    }).collect()
}

/// Polygons whose union is the stroke of a polyline.
pub(crate) fn stroke_polygons(pts: &[Point], closed: bool, s: &Stroke) -> Vec<Vec<Point>> {
    let hw = s.width / 2.0;
    let mut pts: Vec<Point> = pts.to_vec();
    pts.dedup_by(|a, b| dist(*a, *b) < 1e-4);
    if closed && pts.len() > 2 && dist(pts[0], pts[pts.len() - 1]) < 1e-4 {
        pts.pop();
    }
    let n = pts.len();
    let mut out = Vec::new();
    if n < 2 || hw <= 0.0 {
        if n == 1 && s.cap == Cap::Round {
            out.push(disc(pts[0], hw));
        }
        return out;
    }
    let segs = if closed { n } else { n - 1 };
    let dir = |i: usize| {
        let (a, b) = (pts[i % n], pts[(i + 1) % n]);
        let l = dist(a, b).max(1e-6);
        ((b.0 - a.0) / l, (b.1 - a.1) / l)
    };
    for i in 0..segs {
        let (mut a, mut b) = (pts[i], pts[(i + 1) % n]);
        let d = dir(i);
        if !closed && s.cap == Cap::Square {
            if i == 0 { a = (a.0 - d.0 * hw, a.1 - d.1 * hw); }
            if i == segs - 1 { b = (b.0 + d.0 * hw, b.1 + d.1 * hw); }
        }
        let nrm = (-d.1 * hw, d.0 * hw);
        out.push(oriented(vec![(a.0 + nrm.0, a.1 + nrm.1), (b.0 + nrm.0, b.1 + nrm.1), (b.0 - nrm.0, b.1 - nrm.1), (a.0 - nrm.0, a.1 - nrm.1)]));
    }
    // Joins at interior vertices (all of them when closed).
    let joints: Vec<usize> = if closed { (0..n).collect() } else { (1..n - 1).collect() };
    for v in joints {
        let (d1, d2) = (dir((v + n - 1) % n), dir(v));
        let turn = d1.0 * d2.1 - d1.1 * d2.0;
        if turn.abs() < 1e-5 && d1.0 * d2.0 + d1.1 * d2.1 > 0.0 {
            continue; // straight
        }
        let p = pts[v];
        // Outer side is opposite to the turn direction.
        let side = if turn > 0.0 { -1.0 } else { 1.0 };
        let n1 = (-d1.1 * hw * side, d1.0 * hw * side);
        let n2 = (-d2.1 * hw * side, d2.0 * hw * side);
        let (o1, o2) = ((p.0 + n1.0, p.1 + n1.1), (p.0 + n2.0, p.1 + n2.1));
        match s.join {
            Join::Round => out.push(disc(p, hw)),
            Join::Bevel => out.push(oriented(vec![p, o1, o2])),
            Join::Miter(limit) => {
                let cos = (d1.0 * d2.0 + d1.1 * d2.1).clamp(-1.0, 1.0);
                let half = ((1.0 + cos) / 2.0).sqrt(); // cos of half the outer angle
                let m = if half > 1e-4 { 1.0 / half } else { f32::MAX };
                if m <= limit {
                    let bis = (n1.0 + n2.0, n1.1 + n2.1);
                    let bl = (bis.0 * bis.0 + bis.1 * bis.1).sqrt().max(1e-6);
                    let tip = (p.0 + bis.0 / bl * hw * m, p.1 + bis.1 / bl * hw * m);
                    out.push(oriented(vec![p, o1, tip, o2]));
                } else {
                    out.push(oriented(vec![p, o1, o2]));
                }
            }
        }
    }
    if !closed && s.cap == Cap::Round {
        out.push(disc(pts[0], hw));
        out.push(disc(pts[n - 1], hw));
    }
    out
}
