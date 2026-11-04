//! Vector paths: lines, quadratic and cubic Béziers, arcs, polygons, rounded
//! rectangles with a radius per corner, superellipses (squircles).

use std::f32::consts::PI;

pub type Point = (f32, f32);

#[derive(Debug, Clone, Copy, PartialEq)]
enum Cmd {
    Move(Point),
    Line(Point),
    Quad(Point, Point),
    Cubic(Point, Point, Point),
    Close,
}

/// A vector path in pixels (y down). Build it with the methods or the shape
/// constructors; fill or stroke it on a [`super::Canvas`]; test points with
/// [`Path::contains`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Path {
    cmds: Vec<Cmd>,
}

/// Kappa for a quarter circle drawn with one cubic.
const K: f32 = 0.552_284_8;

impl Path {
    pub fn new() -> Path {
        Path::default()
    }

    pub fn move_to(mut self, x: f32, y: f32) -> Self {
        self.cmds.push(Cmd::Move((x, y)));
        self
    }

    pub fn line_to(mut self, x: f32, y: f32) -> Self {
        self.cmds.push(Cmd::Line((x, y)));
        self
    }

    pub fn quad_to(mut self, cx: f32, cy: f32, x: f32, y: f32) -> Self {
        self.cmds.push(Cmd::Quad((cx, cy), (x, y)));
        self
    }

    pub fn cubic_to(mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) -> Self {
        self.cmds.push(Cmd::Cubic((c1x, c1y), (c2x, c2y), (x, y)));
        self
    }

    /// Circular arc around `(cx, cy)` from angle `start` sweeping `sweep`
    /// radians (positive = clockwise on screen). Joins the current point
    /// with a line (or starts a subpath).
    pub fn arc(mut self, cx: f32, cy: f32, r: f32, start: f32, sweep: f32) -> Self {
        let p0 = (cx + r * start.cos(), cy + r * start.sin());
        if matches!(self.cmds.last(), None | Some(Cmd::Close)) {
            self.cmds.push(Cmd::Move(p0));
        } else {
            self.cmds.push(Cmd::Line(p0));
        }
        let parts = (sweep.abs() / (PI / 2.0)).ceil().max(1.0) as usize;
        let step = sweep / parts as f32;
        let k = 4.0 / 3.0 * (step / 4.0).tan();
        let mut a = start;
        for _ in 0..parts {
            let b = a + step;
            let (ca, sa, cb, sb) = (a.cos(), a.sin(), b.cos(), b.sin());
            self.cmds.push(Cmd::Cubic(
                (cx + r * (ca - k * sa), cy + r * (sa + k * ca)),
                (cx + r * (cb + k * sb), cy + r * (sb - k * cb)),
                (cx + r * cb, cy + r * sb),
            ));
            a = b;
        }
        self
    }

    pub fn close(mut self) -> Self {
        self.cmds.push(Cmd::Close);
        self
    }

    /// Appends all subpaths of `other`.
    pub fn append(mut self, other: &Path) -> Self {
        self.cmds.extend_from_slice(&other.cmds);
        self
    }

    // ---- shapes

    pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Path {
        Path::new().move_to(x, y).line_to(x + w, y).line_to(x + w, y + h).line_to(x, y + h).close()
    }

    /// Rectangle with a radius per corner: `[top-left, top-right,
    /// bottom-right, bottom-left]`. Radii are clamped to half the sides.
    pub fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radii: [f32; 4]) -> Path {
        let m = (w.min(h) / 2.0).max(0.0);
        let [tl, tr, br, bl] = radii.map(|r| r.clamp(0.0, m));
        Path::new()
            .move_to(x + tl, y)
            .line_to(x + w - tr, y)
            .cubic_to(x + w - tr + K * tr, y, x + w, y + tr - K * tr, x + w, y + tr)
            .line_to(x + w, y + h - br)
            .cubic_to(x + w, y + h - br + K * br, x + w - br + K * br, y + h, x + w - br, y + h)
            .line_to(x + bl, y + h)
            .cubic_to(x + bl - K * bl, y + h, x, y + h - bl + K * bl, x, y + h - bl)
            .line_to(x, y + tl)
            .cubic_to(x, y + tl - K * tl, x + tl - K * tl, y, x + tl, y)
            .close()
    }

    pub fn circle(cx: f32, cy: f32, r: f32) -> Path {
        Path::ellipse(cx, cy, r, r)
    }

    pub fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> Path {
        Path::new()
            .move_to(cx + rx, cy)
            .cubic_to(cx + rx, cy + K * ry, cx + K * rx, cy + ry, cx, cy + ry)
            .cubic_to(cx - K * rx, cy + ry, cx - rx, cy + K * ry, cx - rx, cy)
            .cubic_to(cx - rx, cy - K * ry, cx - K * rx, cy - ry, cx, cy - ry)
            .cubic_to(cx + K * rx, cy - ry, cx + rx, cy - K * ry, cx + rx, cy)
            .close()
    }

    /// Closed polygon through `points`.
    pub fn polygon(points: &[Point]) -> Path {
        let mut p = Path::new();
        for (i, &(x, y)) in points.iter().enumerate() {
            p = if i == 0 { p.move_to(x, y) } else { p.line_to(x, y) };
        }
        p.close()
    }

    /// Superellipse `|x/a|^n + |y/b|^n = 1` filling the rectangle: `n = 2`
    /// is an ellipse, `n ≈ 4..5` the iOS-like squircle, large `n` a rectangle.
    pub fn squircle(x: f32, y: f32, w: f32, h: f32, n: f32) -> Path {
        let (a, b) = (w / 2.0, h / 2.0);
        let (cx, cy) = (x + a, y + b);
        let e = 2.0 / n.max(0.1);
        let steps = ((a + b) * 0.6).clamp(48.0, 256.0) as usize;
        let pts: Vec<Point> = (0..steps)
            .map(|i| {
                let t = i as f32 / steps as f32 * 2.0 * PI;
                let (c, s) = (t.cos(), t.sin());
                (cx + a * c.signum() * c.abs().powf(e), cy + b * s.signum() * s.abs().powf(e))
            })
            .collect();
        Path::polygon(&pts)
    }

    // ---- transforms

    pub fn translate(&self, dx: f32, dy: f32) -> Path {
        self.map(|(x, y)| (x + dx, y + dy))
    }

    /// Scales around the origin.
    pub fn scale(&self, sx: f32, sy: f32) -> Path {
        self.map(|(x, y)| (x * sx, y * sy))
    }

    /// Rotates `angle` radians around `(cx, cy)`.
    pub fn rotate(&self, angle: f32, cx: f32, cy: f32) -> Path {
        let (c, s) = (angle.cos(), angle.sin());
        self.map(|(x, y)| (cx + (x - cx) * c - (y - cy) * s, cy + (x - cx) * s + (y - cy) * c))
    }

    fn map(&self, f: impl Fn(Point) -> Point) -> Path {
        Path {
            cmds: self
                .cmds
                .iter()
                .map(|c| match *c {
                    Cmd::Move(p) => Cmd::Move(f(p)),
                    Cmd::Line(p) => Cmd::Line(f(p)),
                    Cmd::Quad(a, p) => Cmd::Quad(f(a), f(p)),
                    Cmd::Cubic(a, b, p) => Cmd::Cubic(f(a), f(b), f(p)),
                    Cmd::Close => Cmd::Close,
                })
                .collect(),
        }
    }

    // ---- geometry

    pub fn is_empty(&self) -> bool {
        self.cmds.is_empty()
    }

    /// Bounding box of the control points: `(x0, y0, x1, y1)`.
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        let mut add = |(x, y): Point| {
            b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
        };
        for c in &self.cmds {
            match *c {
                Cmd::Move(p) | Cmd::Line(p) => add(p),
                Cmd::Quad(a, p) => { add(a); add(p) }
                Cmd::Cubic(a, c2, p) => { add(a); add(c2); add(p) }
                Cmd::Close => {}
            }
        }
        if b.0 > b.2 { (0.0, 0.0, 0.0, 0.0) } else { b }
    }

    /// Polylines with curves subdivided to within `tolerance` px.
    /// The bool is true for closed subpaths.
    pub fn flatten(&self, tolerance: f32) -> Vec<(Vec<Point>, bool)> {
        let tol = tolerance.max(0.01);
        let mut out = Vec::new();
        let mut cur: Vec<Point> = Vec::new();
        let mut last = (0.0, 0.0);
        let mut start = (0.0, 0.0);
        let seg_count = |len: f32| ((len / tol).sqrt() * 0.75).ceil().clamp(1.0, 256.0) as usize;
        for c in &self.cmds {
            match *c {
                Cmd::Move(p) => {
                    if cur.len() > 1 {
                        out.push((std::mem::take(&mut cur), false));
                    }
                    cur.clear();
                    cur.push(p);
                    last = p;
                    start = p;
                }
                Cmd::Line(p) => {
                    if cur.is_empty() {
                        cur.push(last);
                    }
                    cur.push(p);
                    last = p;
                }
                Cmd::Quad(a, p) => {
                    if cur.is_empty() {
                        cur.push(last);
                    }
                    let n = seg_count(dist(last, a) + dist(a, p));
                    for i in 1..=n {
                        let t = i as f32 / n as f32;
                        let u = 1.0 - t;
                        cur.push((u * u * last.0 + 2.0 * u * t * a.0 + t * t * p.0, u * u * last.1 + 2.0 * u * t * a.1 + t * t * p.1));
                    }
                    last = p;
                }
                Cmd::Cubic(a, b, p) => {
                    if cur.is_empty() {
                        cur.push(last);
                    }
                    let n = seg_count(dist(last, a) + dist(a, b) + dist(b, p));
                    for i in 1..=n {
                        let t = i as f32 / n as f32;
                        let u = 1.0 - t;
                        let (w0, w1, w2, w3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                        cur.push((w0 * last.0 + w1 * a.0 + w2 * b.0 + w3 * p.0, w0 * last.1 + w1 * a.1 + w2 * b.1 + w3 * p.1));
                    }
                    last = p;
                }
                Cmd::Close => {
                    if cur.len() > 1 {
                        out.push((std::mem::take(&mut cur), true));
                    }
                    cur.clear();
                    last = start;
                }
            }
        }
        if cur.len() > 1 {
            out.push((cur, false));
        }
        out
    }

    /// True when `(x, y)` is inside the filled path (non-zero winding).
    /// Use it to give a non-rectangular widget the right mouse events.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        let mut winding = 0i32;
        for (pts, _) in self.flatten(0.25) {
            let n = pts.len();
            for i in 0..n {
                let (a, b) = (pts[i], pts[(i + 1) % n]);
                if a.1 <= y {
                    if b.1 > y && cross(a, b, (x, y)) > 0.0 {
                        winding += 1;
                    }
                } else if b.1 <= y && cross(a, b, (x, y)) < 0.0 {
                    winding -= 1;
                }
            }
        }
        winding != 0
    }

    /// True when `(x, y)` is within `width / 2` of the path outline (hit
    /// test for stroked shapes such as lines and rings).
    pub fn near_outline(&self, x: f32, y: f32, width: f32) -> bool {
        let r2 = (width / 2.0) * (width / 2.0);
        self.flatten(0.25).iter().any(|(pts, closed)| {
            let n = pts.len();
            let segs = if *closed { n } else { n - 1 };
            (0..segs).any(|i| seg_dist2((x, y), pts[i], pts[(i + 1) % n]) <= r2)
        })
    }
}

pub(crate) fn dist(a: Point, b: Point) -> f32 {
    ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt()
}

fn cross(a: Point, b: Point, p: Point) -> f32 {
    (b.0 - a.0) * (p.1 - a.1) - (p.0 - a.0) * (b.1 - a.1)
}

fn seg_dist2(p: Point, a: Point, b: Point) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0) };
    let (qx, qy) = (a.0 + t * dx, a.1 + t * dy);
    (p.0 - qx).powi(2) + (p.1 - qy).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_and_outline() {
        let r = Path::rounded_rect(0.0, 0.0, 100.0, 50.0, [20.0, 0.0, 0.0, 0.0]);
        assert!(r.contains(50.0, 25.0));
        assert!(!r.contains(2.0, 2.0), "rounded top-left corner is outside");
        assert!(r.contains(98.0, 2.0), "square top-right corner is inside");
        let c = Path::circle(50.0, 50.0, 10.0);
        assert!(c.contains(50.0, 50.0) && !c.contains(59.0, 59.0));
        assert!(c.near_outline(60.0, 50.0, 2.0) && !c.near_outline(50.0, 50.0, 2.0));
        let s = Path::squircle(0.0, 0.0, 100.0, 100.0, 5.0);
        assert!(s.contains(50.0, 50.0) && s.contains(5.0, 50.0) && !s.contains(1.0, 1.0));
        let a = Path::new().arc(0.0, 0.0, 10.0, 0.0, PI).close();
        assert!(a.contains(0.0, 5.0) && !a.contains(0.0, -5.0), "half disc below the x axis");
        assert_eq!(Path::rect(0.0, 0.0, 10.0, 10.0).translate(5.0, 5.0).bounds(), (5.0, 5.0, 15.0, 15.0));
    }
}
