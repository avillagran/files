use crate::{
    model::{Action, Hit, Model},
    painter::{Painter, Rect},
};
use files_core::{listing::Entry, theme::Palette};
use std::{collections::HashSet, path::Path};
const ROW: i32 = 36;
pub fn areas(m: &Model, area: Rect) -> (Rect, Rect) {
    let half = (area.w - 8) / 2;
    let left = Rect::new(area.x, area.y, half, area.h);
    let right = Rect::new(area.x + half + 8, area.y, area.w - half - 8, area.h);
    if m.dual.right_active {
        (right, left)
    } else {
        (left, right)
    }
}
fn target(hits: &mut Vec<Hit>, generation: u64, r: Rect, action: Action) {
    if r.w > 0 && r.h > 0 {
        hits.push(Hit {
            generation,
            rect: r,
            action,
        })
    }
}
#[allow(clippy::too_many_arguments)]
fn list(
    p: &mut dyn Painter,
    area: Rect,
    id: u64,
    revision: u64,
    path: &Path,
    query: &str,
    sort: files_core::listing::Sort,
    descending: bool,
    entries: &[Entry],
    order: &[usize],
    selection: &HashSet<usize>,
    focused: Option<usize>,
    scroll: &mut i32,
    loaded: bool,
    error: &str,
    active: bool,
    palette: &Palette,
    sizes: &crate::size_queue::State,
    generation: u64,
    hits: &mut Vec<Hit>,
) -> Vec<usize> {
    p.rect(
        area,
        if active {
            palette.background
        } else {
            palette.darker_background
        },
    );
    p.rect(
        Rect::new(area.x, area.y, area.w, 2),
        if active {
            palette.accent
        } else {
            palette.lighter_background
        },
    );
    let controls = if query.is_empty() { 28 } else { 56 };
    let title = Rect::new(area.x + 8, area.y + 4, (area.w - 20 - controls).max(0), 30);
    p.text(
        title,
        &if !query.is_empty() && area.w < 300 {
            format!(
                "{}: {query}",
                files_core::name_label::label(path.file_name().unwrap_or(path.as_os_str()))
            )
        } else {
            format!(
                "{} {} · {}",
                crate::fonts::icon("", "DIR"),
                files_core::name_label::label(path.file_name().unwrap_or(path.as_os_str())),
                if query.is_empty() {
                    files_core::name_label::label(path.parent().unwrap_or(path).as_os_str())
                        .into_owned()
                } else {
                    format!("Filtro: {query}")
                }
            )
        },
        12,
        if active {
            palette.accent
        } else {
            palette.foreground
        },
        true,
        false,
    );
    target(hits, generation, title, Action::PaneFocus(id));
    let edit = Rect::new(area.x + area.w - 8 - controls, area.y + 5, 26, 28);
    p.text(
        edit,
        crate::fonts::icon("", "F"),
        14,
        if query.is_empty() {
            palette.dark_foreground
        } else {
            palette.accent
        },
        true,
        false,
    );
    target(
        hits,
        generation,
        edit,
        Action::PaneFilter(id, revision, false),
    );
    if !query.is_empty() {
        let clear = Rect::new(area.x + area.w - 34, area.y + 5, 26, 28);
        p.text(
            clear,
            crate::fonts::icon("", "x"),
            14,
            palette.accent,
            true,
            false,
        );
        target(
            hits,
            generation,
            clear,
            Action::PaneFilter(id, revision, true),
        );
    }
    crate::sort_controls::paint(
        p,
        Rect::new(area.x + 8, area.y + 36, area.w - 16, 24),
        palette,
        hits,
        crate::sort_controls::Header {
            sort,
            descending,
            generation,
            pane: Some((id, revision)),
            query: "",
        },
    );
    let body = Rect::new(area.x, area.y + 64, area.w, (area.h - 92).max(0));
    if loaded {
        *scroll = (*scroll).clamp(0, (order.len() as i32 * ROW - body.h).max(0));
    }
    target(hits, generation, body, Action::PaneBackground(id, revision));
    p.clip(body);
    let first = (*scroll / ROW).max(0) as usize;
    let count = (body.h / ROW + 2) as usize;
    let mut visible = Vec::with_capacity(count);
    let columns =
        crate::list_columns::Layout::new((area.w - 6).max(0), crate::list_columns::Profile::Dual);
    for (rank, &i) in order.iter().enumerate().skip(first).take(count) {
        let Some(e) = entries.get(i) else { continue };
        let y = body.y + rank as i32 * ROW - *scroll;
        let r = Rect::new(area.x + 3, y, area.w - 6, ROW - 2);
        if selection.contains(&i) {
            p.rounded_rect(
                r,
                4,
                if active {
                    palette.selection
                } else {
                    palette.lighter_background
                },
            );
        }
        if focused == Some(i) && active {
            p.rect(Rect::new(r.x, r.y, 2, r.h), palette.accent);
        }
        p.text(
            Rect::new(r.x + 6, y, 22, ROW - 2),
            if e.is_dir() {
                crate::fonts::icon("", "D")
            } else {
                crate::fonts::icon("", "F")
            },
            15,
            if e.is_dir() {
                palette.accent
            } else {
                palette.dark_foreground
            },
            true,
            false,
        );
        p.text(
            Rect::new(r.x + columns.name.x, y, columns.name.w, ROW - 2),
            &files_core::name_label::label(&e.name),
            12,
            palette.foreground,
            true,
            false,
        );
        if let Some(size) = columns.size {
            p.text(
                Rect::new(r.x + size.x, y, size.w, ROW - 2),
                &if e.is_dir() {
                    sizes
                        .label(path, &e.name)
                        .unwrap_or_else(|| "Carpeta".into())
                } else {
                    e.metadata
                        .as_ref()
                        .map(|v| files_core::format_size(v.size))
                        .unwrap_or_else(|| "…".into())
                },
                11,
                palette.dark_foreground,
                true,
                true,
            );
        }
        let top = r.y.max(body.y);
        let bottom = (r.y + r.h).min(body.y + body.h);
        if bottom > top {
            target(
                hits,
                generation,
                Rect::new(r.x, top, r.w, bottom - top),
                Action::PaneEntry(id, revision, i),
            );
            visible.push(i);
        }
    }
    if order.is_empty() {
        p.text(
            Rect::new(area.x + 12, body.y + 12, area.w - 24, 56),
            if !loaded {
                "Cargando…"
            } else if !error.is_empty() {
                error
            } else {
                "Sin elementos"
            },
            12,
            palette.dark_foreground,
            true,
            false,
        );
    }
    p.unclip();
    let label = if !error.is_empty() {
        error.to_owned()
    } else if !query.is_empty() {
        if area.w < 300 {
            format!(
                "{} {}/{} · {} marc.",
                crate::fonts::icon("", "F"),
                order.len(),
                entries.len(),
                selection.len()
            )
        } else {
            format!(
                "{} visibles / {} cargados · {} marcados · Filtro: {query}",
                order.len(),
                entries.len(),
                selection.len()
            )
        }
    } else if area.w < 300 {
        format!(
            "{}/{} · {} marc.",
            order.len(),
            entries.len(),
            selection.len()
        )
    } else {
        format!(
            "{} visibles / {} cargados · {} marcados{}",
            order.len(),
            entries.len(),
            selection.len(),
            if active && area.w >= 400 {
                " · Tab cambia panel"
            } else {
                ""
            }
        )
    };
    p.text(
        Rect::new(area.x + 8, area.y + area.h - 26, area.w - 16, 24),
        &label,
        10,
        if error.is_empty() {
            palette.dark_foreground
        } else {
            palette.red
        },
        true,
        false,
    );
    visible
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, area: Rect) {
    let (active, other) = areas(m, area);
    let palette = m.palette.clone();
    m.visible = list(
        p,
        active,
        m.dual.active_id,
        m.pane_revision,
        &m.path,
        &m.query,
        m.sort,
        m.descending,
        &m.entries,
        &m.order,
        &m.selection,
        m.selected,
        &mut m.scroll,
        m.loaded,
        &m.error,
        true,
        &palette,
        &m.sizes,
        m.generation,
        &mut m.hits,
    );
    m.content_height = m.order.len() as i32 * ROW + 92;
    if let Some(b) = &mut m.dual.other {
        let visible = list(
            p,
            other,
            b.id,
            b.revision,
            &b.path,
            &b.query,
            b.sort,
            b.descending,
            &b.entries,
            &b.order,
            &b.selection,
            b.selected,
            &mut b.scroll,
            b.loaded,
            &b.error,
            false,
            &palette,
            &m.sizes,
            m.generation,
            &mut m.hits,
        );
        b.request_stats(&visible);
    }
}
pub fn reveal(m: &mut Model, h: i32) {
    if let Some(rank) = m
        .selected
        .and_then(|i| m.order.iter().position(|j| *j == i))
    {
        let y = rank as i32 * ROW;
        let available = (crate::operation_bar::browser_height(m, h) - 230).max(ROW);
        if y < m.scroll {
            m.scroll = y;
        } else if y + ROW > m.scroll + available {
            m.scroll = (y + ROW - available).max(0);
        }
    }
}

#[cfg(test)]
mod narrow_name_tests {
    use super::*;
    use files_core::{
        listing::{Kind, Metadata, Sort},
        theme::Color,
    };
    #[derive(Default)]
    struct Recorder(Vec<(Rect, String)>);
    impl Painter for Recorder {
        fn rect(&mut self, _: Rect, _: Color) {}
        fn rounded_rect(&mut self, _: Rect, _: i32, _: Color) {}
        fn text(&mut self, r: Rect, t: &str, _: i32, _: Color, _: bool, _: bool) {
            self.0.push((r, t.into()));
        }
        fn image(&mut self, _: Rect, _: &crate::painter::Image<'_>) {}
        fn clip(&mut self, _: Rect) {}
        fn unclip(&mut self) {}
    }
    #[test]
    fn compact_rows_paint_names_in_both_panes_without_metadata_or_target_overlap() {
        let mut names = vec![std::ffi::OsString::from("one.txt"), "two.txt".into()];
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            names[1] = std::ffi::OsString::from_vec(b"a\xff\n".to_vec());
        }
        let entries = names
            .iter()
            .map(|name| Entry {
                name: name.clone(),
                kind: Kind::File,
                metadata: Some(Metadata {
                    size: 7,
                    modified: std::time::UNIX_EPOCH,
                }),
                stat_attempted: true,
            })
            .collect::<Vec<_>>();
        let palette = Palette::parse(include_str!("../../../themes/tokyo-night.toml")).unwrap();
        for width in [123, 180, 400, 800] {
            for active in [false, true] {
                let mut p = Recorder::default();
                let mut hits = vec![];
                let mut scroll = 0;
                let visible = list(
                    &mut p,
                    Rect::new(10, 20, width, 240),
                    31,
                    7,
                    Path::new("/fixture-narrow"),
                    "",
                    Sort::Size,
                    false,
                    &entries,
                    &[1, 0],
                    &HashSet::from([1]),
                    Some(1),
                    &mut scroll,
                    true,
                    "",
                    active,
                    &palette,
                    &Default::default(),
                    9,
                    &mut hits,
                );
                assert_eq!(visible, vec![1, 0]);
                for name in &names {
                    let label = files_core::name_label::label(name);
                    let r = p.0.iter().find(|(_, t)| t == label.as_ref()).unwrap().0;
                    assert!(r.w >= 80 && r.x >= 10 && r.x + r.w <= 10 + width);
                    assert!(p.text_width(&label, 12, true) <= r.w as f32);
                    if width < 200 {
                        assert!(
                            !p.0.iter()
                                .any(|(other, _)| other.y == r.y && other.x >= r.x + r.w)
                        );
                    }
                }
                assert_eq!(
                    hits.iter()
                        .filter(|h| matches!(h.action, Action::PaneEntry(31, 7, _))
                            && h.generation == 9)
                        .count(),
                    2
                );
                let sorts = hits
                    .iter()
                    .filter(|h| matches!(h.action, Action::PaneSort(31, 7, _)))
                    .collect::<Vec<_>>();
                assert_eq!(sorts.len(), 4);
                for (i, a) in sorts.iter().enumerate() {
                    assert!(a.rect.w > 0 && a.rect.x >= 10 && a.rect.x + a.rect.w <= 10 + width);
                    for b in &sorts[i + 1..] {
                        assert!(a.rect.x + a.rect.w <= b.rect.x || b.rect.x + b.rect.w <= a.rect.x);
                    }
                }
                assert!(p.0.iter().any(|(_, t)| t == "7 B") == (width >= 400));
            }
        }
        assert_eq!(
            entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>(),
            names
        );
    }
}
