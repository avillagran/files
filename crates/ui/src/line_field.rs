//! Bounded single-line geometry and exact draft cache. No native focus or clipboard.
use crate::{
    line_edit::Cursor,
    painter::{Painter, Rect},
};
use files_core::theme::Palette;
#[derive(Default)]
pub struct Geometry {
    area: Rect,
    points: Vec<(usize, f32)>,
    cached: String,
    scroll: f32,
    stamp: Option<u64>,
}
impl Geometry {
    pub fn reveal_end(&mut self) {
        self.scroll = f32::MAX;
    }
    pub fn paint(
        &mut self,
        p: &mut dyn Painter,
        text: &str,
        cursor: &Cursor,
        area: Rect,
        colors: &Palette,
        focus: (bool, u64),
    ) {
        self.paint_bounded(
            p,
            text,
            cursor,
            (area, crate::omnibar_input::LIMIT),
            colors,
            focus,
        );
    }
    pub fn paint_bounded(
        &mut self,
        p: &mut dyn Painter,
        text: &str,
        cursor: &Cursor,
        bounds: (Rect, usize),
        colors: &Palette,
        focus: (bool, u64),
    ) {
        let (area, limit) = bounds;
        let (active, epoch) = focus;
        let resized = self.area.w != area.w || self.area.h != area.h;
        self.area = area;
        // Ordinary fields keep4096B; URI editors supply their derived bound.
        // Never allocate geometry for a draft outside its caller-specific limit.
        if text.len() > limit {
            self.stamp = None;
            return;
        }
        // Fixed logical Courier13 metrics. Resize invalidates geometry as well as
        // exact draft changes; cursor/progress-only paints reuse measured advances.
        if resized || self.cached != text || self.points.is_empty() {
            self.points.clear();
            self.cached.clear();
            if self.cached.capacity() < text.len() {
                self.cached.reserve_exact(text.len());
            }
            self.cached.push_str(text);
            let wanted = text.chars().count() + 1;
            if self.points.capacity() < wanted {
                self.points.reserve_exact(wanted);
            }
            self.points.push((0, 0.));
            let mut width = 0.;
            for (i, c) in text.char_indices() {
                let mut buf = [0; 4];
                width += p.text_width(c.encode_utf8(&mut buf), 13, true).max(0.);
                self.points.push((i + c.len_utf8(), width));
            }
        }
        let width = self.points.last().map_or(0., |(_, x)| *x);
        let at = |byte| {
            self.points
                .iter()
                .find(|(b, _)| *b == byte)
                .map_or(width, |(_, x)| *x)
        };
        let caret = at(cursor.caret);
        let selection = cursor.range(text);
        let a = at(selection.start);
        let b = at(selection.end);
        let available = (area.w - 16).max(1) as f32;
        if active {
            if caret < self.scroll {
                self.scroll = caret;
            }
            if caret > self.scroll + available {
                self.scroll = caret - available;
            }
        }
        self.scroll = self.scroll.clamp(0., (width - available).max(0.));
        let x = area.x + 8 - self.scroll.round() as i32;
        p.clip(area);
        if active && !selection.is_empty() {
            p.rect(
                Rect::new(
                    x + a.round() as i32,
                    area.y + 5,
                    (b - a).round() as i32,
                    area.h - 10,
                ),
                colors.selection,
            );
        }
        p.literal_text(
            Rect::new(x, area.y, (width.ceil() as i32 + 2).max(1), area.h),
            text,
            13,
            colors.accent,
        );
        if active {
            p.rect(
                Rect::new(x + caret.round() as i32, area.y + 5, 1, area.h - 10),
                colors.accent,
            );
        }
        p.unclip();
        self.stamp = Some(epoch);
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        self.area.contains(x, y)
    }
    pub fn position(&self, x: i32, epoch: u64) -> Option<usize> {
        if self.stamp != Some(epoch) {
            return None;
        }
        let x = (x - self.area.x - 8) as f32 + self.scroll;
        let i = self.points.partition_point(|(_, p)| *p < x);
        if i == 0 {
            self.points.first().map(|(b, _)| *b)
        } else if i >= self.points.len() {
            self.points.last().map(|(b, _)| *b)
        } else {
            Some(if x - self.points[i - 1].1 <= self.points[i].1 - x {
                self.points[i - 1].0
            } else {
                self.points[i].0
            })
        }
    }
    pub fn painted_position(&self, byte: usize, epoch: u64) -> Option<(i32, i32)> {
        if self.stamp != Some(epoch) {
            return None;
        }
        let x = self.area.x
            + 8
            + (self.points.iter().find(|(b, _)| *b == byte)?.1 - self.scroll).round() as i32;
        let y = self.area.y + self.area.h / 2;
        self.area.contains(x, y).then_some((x, y))
    }
    #[cfg(test)]
    pub fn storage(&self) -> usize {
        self.points.capacity() * std::mem::size_of::<(usize, f32)>()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct TestPainter {
        width_calls: usize,
    }
    impl Painter for TestPainter {
        fn rect(&mut self, _: Rect, _: files_core::theme::Color) {}
        fn rounded_rect(&mut self, _: Rect, _: i32, _: files_core::theme::Color) {}
        fn text(
            &mut self,
            _: Rect,
            _: &str,
            _: i32,
            _: files_core::theme::Color,
            _: bool,
            _: bool,
        ) {
        }
        fn text_width(&mut self, text: &str, size: i32, _: bool) -> f32 {
            self.width_calls += 1;
            text.chars().count() as f32 * size as f32 * 0.6
        }
        fn image(&mut self, _: Rect, _: &crate::painter::Image<'_>) {}
        fn clip(&mut self, _: Rect) {}
        fn unclip(&mut self) {}
    }
    #[test]
    fn progress_and_caret_only_paints_reuse_exact_geometry_and_resize_rebuilds() {
        let mut p = TestPainter::default();
        let mut s = crate::search_state::State::default();
        s.open("/fixture".into(), "日本.md".into(), false);
        let colors = Palette::default();
        let area = Rect::new(0, 0, 120, 30);
        s.paint_input(&mut p, 0, area, &colors);
        let measured = p.width_calls;
        assert_eq!(measured, 5);
        s.motion(crate::line_edit::Motion::Start, false);
        s.paint_input(&mut p, 0, area, &colors);
        assert_eq!(p.width_calls, measured);
        s.paint_input(&mut p, 0, Rect::new(0, 0, 100, 30), &colors);
        assert_eq!(p.width_calls, 2 * measured);
        s.input("x");
        s.paint_input(&mut p, 0, area, &colors);
        assert!(p.width_calls > 2 * measured);
    }
    #[test]
    fn unicode_geometry_scroll_and_old_pointer_proof_are_bounded() {
        use crate::line_edit::Motion;
        let mut s = crate::search_state::State::default();
        s.open("/fixture".into(), "日本 old.txt".into(), false);
        s.motion(Motion::Start, false);
        let area = Rect::new(10, 10, 120, 32);
        let mut p = TestPainter::default();
        let colors = Palette::default();
        s.paint_input(&mut p, 0, area, &colors);
        let (x, y) = s.geometry[0]
            .painted_position(0, s.input_revision())
            .unwrap();
        assert!(s.pointer(x, y, true, false, false, false));
        s.paint_input(&mut p, 0, area, &colors);
        let (x, y) = s.geometry[0]
            .painted_position(6, s.input_revision())
            .unwrap();
        assert!(s.pointer(x, y, false, true, false, false));
        assert_eq!(s.selected_text(), "日本");
        let epoch = s.input_revision();
        s.motion(Motion::End, false);
        assert!(s.geometry[0].position(x, s.input_revision()).is_none());
        assert!(s.input_revision() != epoch);
        s.focus(0);
        s.input(&"x".repeat(4096));
        s.paint_input(&mut p, 0, area, &colors);
        assert!(
            s.geometry[0]
                .painted_position(4096, s.input_revision())
                .is_some()
        );
        assert!(
            s.geometry[0]
                .painted_position(0, s.input_revision())
                .is_none()
        );
        assert!(s.geometry[0].storage() <= 4097 * std::mem::size_of::<(usize, f32)>());
        s.close();
        assert_eq!(s.geometry[0].storage(), 0);
    }
}
