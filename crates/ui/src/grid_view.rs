//! Auto-arranged icon grid, distinct from persistent free-position Spatial.
//! Only visible rows produce draw commands/hits and bounded thumbnail demand; no UI I/O.
use crate::{
    model::{Action, Hit, Model},
    painter::{Painter, Rect},
};
use std::ops::Range;
const HEADER: i32 = 36;
#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    pub columns: usize,
    pub cell_width: i32,
    pub cell_height: i32,
    pub content_height: i32,
}
impl Metrics {
    pub fn new(width: i32, zoom: f32, count: usize) -> Self {
        let z = if zoom.is_finite() {
            zoom.clamp(0.65, 1.8)
        } else {
            1.
        };
        let columns = (width.max(1) / (156. * z).round().max(1.) as i32).max(1) as usize;
        let cell_width = (width.max(1) / columns as i32).max(1);
        let cell_height = (132. * z).round().max(1.) as i32;
        let rows = count.div_ceil(columns);
        let content_height = rows
            .saturating_mul(cell_height as usize)
            .saturating_add(HEADER as usize)
            .min(i32::MAX as usize) as i32;
        Self {
            columns,
            cell_width,
            cell_height,
            content_height,
        }
    }
    pub fn visible(&self, count: usize, scroll: i32, height: i32) -> Range<usize> {
        let first = (scroll.max(0) as usize / self.cell_height as usize)
            .saturating_mul(self.columns)
            .min(count);
        let end = (scroll.max(0) as usize)
            .saturating_add((height - HEADER).max(0) as usize)
            .div_ceil(self.cell_height as usize)
            .saturating_mul(self.columns)
            .min(count);
        first..end.max(first)
    }
    pub fn row_top(&self, rank: usize) -> i32 {
        (rank / self.columns)
            .saturating_mul(self.cell_height as usize)
            .saturating_add(HEADER as usize)
            .min(i32::MAX as usize) as i32
    }
}
pub fn width(m: &Model, w: i32) -> i32 {
    w - if m.inspector && w >= 800 { 240 } else { 0 } - 16 - if m.rail { 186 } else { 68 }
}
pub fn move_focus(m: &mut Model, delta: i32, extend: bool, w: i32, h: i32) {
    if m.order.is_empty() {
        return;
    }
    let rank = m
        .selected
        .and_then(|i| m.order.iter().position(|&j| i == j))
        .unwrap_or(0);
    let next =
        (rank as i64 + i64::from(delta)).clamp(0, m.order.len().saturating_sub(1) as i64) as usize;
    m.select_entry(m.order[next], false, extend);
    reveal(m, w, h);
}
pub fn reveal(m: &mut Model, w: i32, h: i32) {
    let Some(rank) = m
        .selected
        .and_then(|i| m.order.iter().position(|&j| i == j))
    else {
        return;
    };
    let metrics = Metrics::new(width(m, w), m.grid_zoom, m.order.len());
    let y = metrics.row_top(rank) - HEADER;
    let available = (crate::operation_bar::browser_height(m, h) - 134 - HEADER).max(1);
    if y < m.scroll {
        m.scroll = y;
    } else if y + metrics.cell_height > m.scroll + available {
        m.scroll = (y + metrics.cell_height - available).max(0);
    }
}
fn control(p: &mut dyn Painter, m: &mut Model, r: Rect, label: &str, action: Action) {
    p.rounded_rect(r, 5 * m.radius, m.palette.lighter_background);
    p.text(r, label, 13, m.palette.accent, true, false);
    m.hits.push(Hit {
        rect: r,
        action,
        generation: m.generation,
    });
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, body: Rect) {
    let metrics = Metrics::new(body.w, m.grid_zoom, m.order.len());
    m.content_height = metrics.content_height.max(body.h);
    m.clamp_scroll(body.h);
    let caption = m.thumbnails.notice.clone().unwrap_or_else(|| {
        format!(
            "Grilla · {} · {}/{} miniaturas (máx32) · {} errores",
            m.order.len(),
            m.thumbnails.ready(),
            m.thumbnails.count(),
            m.thumbnails.errors()
        )
    });
    p.text(
        Rect::new(body.x, body.y, body.w - 160, 28),
        &caption,
        12,
        if m.thumbnails.notice.is_some() {
            m.palette.red
        } else {
            m.palette.dark_foreground
        },
        true,
        false,
    );
    p.text(
        Rect::new(body.x + body.w - 156, body.y, 64, 28),
        &format!("{:.0}%", m.grid_zoom * 100.),
        11,
        m.palette.dark_foreground,
        true,
        true,
    );
    control(
        p,
        m,
        Rect::new(body.x + body.w - 84, body.y, 36, 28),
        "−",
        Action::GridZoom(1. / 1.15),
    );
    control(
        p,
        m,
        Rect::new(body.x + body.w - 40, body.y, 36, 28),
        "+",
        Action::GridZoom(1.15),
    );
    let clip = Rect::new(body.x, body.y + HEADER, body.w, (body.h - HEADER).max(0));
    p.clip(clip);
    for rank in metrics.visible(m.order.len(), m.scroll, body.h) {
        let i = m.order[rank];
        let e = &m.entries[i];
        let cell = Rect::new(
            body.x + (rank % metrics.columns) as i32 * metrics.cell_width,
            body.y + metrics.row_top(rank) - m.scroll,
            metrics.cell_width,
            metrics.cell_height,
        );
        let card = Rect::new(
            cell.x + 4,
            cell.y + 3,
            (cell.w - 8).max(0),
            (cell.h - 6).max(0),
        );
        let on = m.selection.contains(&i) || m.selected == Some(i);
        if on || m.hovered == Some(i) {
            p.rounded_rect(
                card,
                8 * m.radius,
                if on {
                    m.palette.selection
                } else {
                    m.palette.lighter_background
                },
            );
        }
        if m.selected == Some(i) {
            p.rect(
                Rect::new(card.x + 3, card.y + 6, 2, (card.h - 12).max(0)),
                m.palette.accent,
            );
        }
        let mime = files_core::mime::mime(std::path::Path::new(&e.name));
        let (glyph, color) = if e.is_dir() {
            ("", m.palette.blue)
        } else if mime.starts_with("image/") {
            ("", m.palette.magenta)
        } else if mime.starts_with("video/") {
            ("", m.palette.magenta)
        } else if mime.starts_with("audio/") {
            ("", m.palette.green)
        } else if mime == "application/pdf" {
            ("", m.palette.red)
        } else {
            ("", m.palette.cyan)
        };
        let icon_size = (36. * m.grid_zoom).round() as i32;
        let icon_width = p
            .text_width(crate::fonts::icon(glyph, "FILE"), icon_size, true)
            .max(icon_size as f32)
            .ceil() as i32;
        let icon_x = card.x + (card.w - icon_width) / 2;
        p.clip(card);
        let icon_h = (cell.h - 55).max(1);
        let path = m.path.join(&e.name);
        let mut palette = m.palette.clone();
        palette.background = if on {
            m.palette.selection
        } else if m.hovered == Some(i) {
            m.palette.lighter_background
        } else {
            m.palette.background
        };
        if !m.thumbnails.paint(
            p,
            &path,
            Rect::new(card.x + 4, card.y + 3, card.w - 8, icon_h - 4),
            &palette,
        ) {
            p.text(
                Rect::new(
                    icon_x,
                    card.y + 2,
                    (card.x + card.w - icon_x - 4).max(1),
                    icon_h,
                ),
                crate::fonts::icon(glyph, "FILE"),
                icon_size,
                color,
                true,
                false,
            );
        }
        p.text(
            Rect::new(card.x + 7, card.y + icon_h, card.w - 14, 24),
            &files_core::name_label::label(&e.name),
            11,
            m.palette.bright_foreground,
            true,
            false,
        );
        let detail = if e.is_dir() {
            m.sizes
                .label(&m.path, &e.name)
                .unwrap_or_else(|| "Carpeta".into())
        } else {
            e.metadata
                .as_ref()
                .map(|meta| files_core::format_size(meta.size))
                .unwrap_or_else(|| "—".into())
        };
        p.text(
            Rect::new(card.x + 7, card.y + icon_h + 23, card.w - 14, 20),
            &detail,
            10,
            m.palette.dark_foreground,
            true,
            false,
        );
        p.unclip();
        let y = cell.y.max(clip.y);
        let bottom = (cell.y + cell.h).min(clip.y + clip.h);
        if bottom > y {
            m.hits.push(Hit {
                rect: Rect::new(cell.x, y, cell.w, bottom - y),
                action: Action::Entry(i),
                generation: m.generation,
            });
            m.visible.push(i);
        }
    }
    p.unclip();
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visible_work_is_bounded_by_viewport_for_100k_and_scroll_end() {
        for zoom in [0.65, 1., 1.8] {
            for width in [200, 680, 1400] {
                let m = Metrics::new(width, zoom, 100_000);
                for scroll in [0, 37, m.content_height - 400] {
                    let r = m.visible(100_000, scroll, 400);
                    assert!(r.end <= 100_000);
                    assert!(r.len() <= m.columns * ((400 / m.cell_height + 2) as usize));
                    assert!(r.start <= r.end);
                }
            }
        }
    }
    #[test]
    fn resize_zoom_empty_and_extreme_dimensions_do_not_keep_per_entry_state() {
        let a = Metrics::new(700, 1., 100);
        let b = Metrics::new(350, 1., 100);
        assert!(a.columns > b.columns);
        assert!(a.content_height < b.content_height);
        assert!(Metrics::new(700, 1.8, 100).columns < a.columns);
        assert!(Metrics::new(0, f32::NAN, 0).visible(0, 0, 0).is_empty());
        let max = Metrics::new(1, 1., usize::MAX);
        assert_eq!(max.content_height, i32::MAX);
        assert!(max.visible(usize::MAX, i32::MAX, i32::MAX).len() < 100_000_000);
    }
    struct Recorder {
        commands: usize,
        clips: i32,
    }
    impl Painter for Recorder {
        fn rect(&mut self, _: Rect, _: files_core::theme::Color) {
            self.commands += 1;
        }
        fn rounded_rect(&mut self, _: Rect, _: i32, _: files_core::theme::Color) {
            self.commands += 1;
        }
        fn text(
            &mut self,
            _: Rect,
            _: &str,
            _: i32,
            _: files_core::theme::Color,
            _: bool,
            _: bool,
        ) {
            self.commands += 1;
        }
        fn image(&mut self, _: Rect, _: &crate::painter::Image<'_>) {
            panic!("Icon grid must not decode images");
        }
        fn clip(&mut self, _: Rect) {
            self.clips += 1;
        }
        fn unclip(&mut self) {
            self.clips -= 1;
            assert!(self.clips >= 0);
        }
    }
    #[test]
    fn paint_selection_range_hidden_and_zoom_share_real_model_without_layout_writes() {
        use crate::{aux_io::AuxIo, model::View};
        use files_core::listing::{Entry, Kind};
        let root = files_core::theme::home()
            .join(".cache/files-test")
            .join(format!("grid-model-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut m = Model::new(root.clone());
        let (tx, _) = std::sync::mpsc::sync_channel(4);
        let (_, rx) = std::sync::mpsc::channel();
        m.aux = AuxIo { tx, rx };
        m.inspector = false;
        m.show_hidden = false;
        m.loaded = true;
        m.entries = (0..1000)
            .map(|n| Entry {
                name: format!("item{n}.txt").into(),
                kind: Kind::File,
                metadata: None,
                stat_attempted: false,
            })
            .collect();
        m.entries.push(Entry {
            name: ".hidden".into(),
            kind: Kind::File,
            metadata: None,
            stat_attempted: false,
        });
        m.action(Action::View(View::Grid));
        let original_layout = std::sync::Arc::clone(&m.layout);
        let mut p = Recorder {
            commands: 0,
            clips: 0,
        };
        paint(&mut p, &mut m, Rect::new(68, 62, 916, 458));
        assert!(m.visible.len() < 50 && p.commands < 200 && p.clips == 0);
        assert!(
            m.hits
                .iter()
                .all(|hit| !matches!(hit.action, Action::Entry(1000)))
        );
        m.select_entry(m.order[0], false, false);
        move_focus(&mut m, 8, true, 1000, 560);
        assert_eq!(m.selection.len(), 9);
        move_focus(&mut m, 500, false, 1000, 560);
        assert!(m.scroll > 0 && m.selection.len() == 1);
        m.hits.clear();
        m.visible.clear();
        paint(&mut p, &mut m, Rect::new(68, 62, 916, 458));
        assert!(m.visible.contains(&m.selected.unwrap()));
        m.action(Action::GridZoom(100.));
        assert_eq!(m.grid_zoom, 1.8);
        assert_eq!(m.scroll, 0);
        m.action(Action::GridZoom(f32::NAN));
        assert_eq!(m.grid_zoom, 1.8);
        assert!(std::sync::Arc::ptr_eq(&m.layout, &original_layout) && !m.layout_dirty);
        m.action(Action::View(View::List));
        assert!(m.hits.is_empty() && m.visible.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn painting_between_tab_restore_and_done_or_layout_keeps_scroll() {
        use crate::{model::View, tabs::Command};
        use files_core::listing::{Entry, Kind};
        let entries = || {
            (0..1000)
                .map(|n| Entry {
                    name: format!("item{n}.txt").into(),
                    kind: Kind::File,
                    metadata: None,
                    stat_attempted: false,
                })
                .collect::<Vec<_>>()
        };
        let mut m = Model::new("/owned-paint-fixture-not-created".into());
        m.view = View::Grid;
        m.loaded = true;
        m.entries = entries();
        m.rebuild();
        m.scroll = 120;
        let a = m.tabs.active;
        m.tab_command(Command::New);
        m.tab_command(Command::Select(a));
        assert!(!m.loaded && m.entries.is_empty());
        let mut p = Recorder {
            commands: 0,
            clips: 0,
        };
        let body = Rect::new(68, 94, 676, 516);
        paint(&mut p, &mut m, body);
        m.clamp_scroll(body.h);
        assert_eq!(m.scroll, 120);
        m.entries = entries();
        m.loaded = true;
        m.rebuild();
        paint(&mut p, &mut m, body);
        m.clamp_scroll(body.h);
        assert_eq!(m.scroll, 120);
        m.entries.truncate(1);
        m.rebuild();
        m.view = View::Spatial;
        m.layout_ready = false;
        m.scroll = 5000;
        crate::spatial_view::paint(&mut p, &mut m, body);
        m.clamp_scroll(body.h);
        assert_eq!(m.scroll, 5000);
        let name = m.entries[0].name.clone();
        std::sync::Arc::make_mut(&mut m.layout)
            .set_positions(&[(
                name.as_os_str(),
                files_core::spatial::Position { x: 0., y: 6000. },
            )])
            .unwrap();
        m.layout_ready = true;
        crate::spatial_view::paint(&mut p, &mut m, body);
        m.clamp_scroll(body.h);
        assert_eq!(m.scroll, 5000);
        m.view = View::SpaceMap;
        m.scroll = 120;
        m.space_map.open(m.path.clone());
        crate::space_map_view::paint(&mut p, &mut m, body);
        m.clamp_scroll(body.h);
        assert_eq!(m.scroll, 120);
        m.space_map.close();
        assert_eq!(p.clips, 0);
    }
}
