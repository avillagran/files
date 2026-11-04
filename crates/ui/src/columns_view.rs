use crate::{
    columns::Data,
    model::{Action, Hit, Model},
    painter::{Painter, Rect},
};
use files_core::{listing::Entry, theme::Palette};
use std::collections::HashSet;
pub const WIDTH: i32 = 260;
const ROW: i32 = 36;
fn hit(hits: &mut Vec<Hit>, rect: Rect, generation: u64, action: Action) {
    if rect.w > 0 && rect.h > 0 {
        hits.push(Hit {
            rect,
            generation,
            action,
        });
    }
}
fn clipped(r: Rect, area: Rect) -> Rect {
    let x = r.x.max(area.x);
    let y = r.y.max(area.y);
    Rect::new(
        x,
        y,
        (r.x + r.w).min(area.x + area.w).saturating_sub(x).max(0),
        (r.y + r.h).min(area.y + area.h).saturating_sub(y).max(0),
    )
}
struct List<'a> {
    entries: &'a [Entry],
    order: &'a [usize],
    selection: &'a HashSet<usize>,
    selected: Option<usize>,
    query: &'a str,
    scroll: i32,
    loaded: bool,
    error: &'a str,
}
impl<'a> From<&'a Data> for List<'a> {
    fn from(d: &'a Data) -> Self {
        Self {
            entries: &d.entries,
            order: &d.order,
            selection: &d.selection,
            selected: d.selected,
            query: &d.query,
            scroll: d.scroll,
            loaded: d.loaded,
            error: &d.error,
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn column(
    p: &mut dyn Painter,
    list: List<'_>,
    path: &std::path::Path,
    id: u64,
    active: bool,
    rect: Rect,
    viewport: Rect,
    palette: &Palette,
    sizes: &crate::size_queue::State,
    generation: u64,
    hits: &mut Vec<Hit>,
    visible: &mut Vec<usize>,
) {
    let clip = clipped(rect, viewport);
    if clip.w == 0 {
        return;
    }
    p.clip(clip);
    p.rect(
        rect,
        if active {
            palette.background
        } else {
            palette.darker_background
        },
    );
    p.rect(
        Rect::new(rect.x, rect.y, rect.w, 2),
        if active {
            palette.accent
        } else {
            palette.lighter_background
        },
    );
    p.rect(
        Rect::new(rect.x + rect.w - 1, rect.y, 1, rect.h),
        palette.lighter_background,
    );
    let title = files_core::name_label::label(path.file_name().unwrap_or(path.as_os_str()));
    p.text(
        Rect::new(rect.x + 10, rect.y + 4, rect.w - 20, 26),
        &format!("{}  {}", crate::fonts::icon("", "DIR"), title),
        12,
        if active {
            palette.accent
        } else {
            palette.foreground
        },
        true,
        false,
    );
    hit(
        hits,
        clipped(Rect::new(rect.x, rect.y, rect.w, 32), viewport),
        generation,
        Action::ColumnFocus(id),
    );
    let rows = Rect::new(rect.x, rect.y + 34, rect.w, rect.h - 64);
    p.clip(rows);
    let first = (list.scroll / ROW).max(0) as usize;
    let count = (rows.h.max(0) / ROW + 2) as usize;
    for rank in first..list.order.len().min(first + count) {
        let i = list.order[rank];
        let Some(e) = list.entries.get(i) else {
            continue;
        };
        let y = rows.y + rank as i32 * ROW - list.scroll;
        let r = Rect::new(rect.x + 3, y, rect.w - 6, ROW - 2);
        let selected = list.selection.contains(&i);
        if selected {
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
        if list.selected == Some(i) && active {
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
        let size = if e.is_dir() {
            sizes.label(path, &e.name)
        } else {
            None
        };
        p.text(
            Rect::new(
                r.x + 30,
                y,
                r.w - if size.is_some() { 130 } else { 58 },
                ROW - 2,
            ),
            &files_core::name_label::label(&e.name),
            12,
            palette.foreground,
            true,
            false,
        );
        if let Some(size) = size {
            p.text(
                Rect::new(r.x + r.w - 94, y, 68, ROW - 2),
                &size,
                10,
                palette.accent,
                true,
                true,
            );
        }
        if e.is_dir() {
            p.text(
                Rect::new(r.x + r.w - 22, y, 16, ROW - 2),
                crate::fonts::icon("", ">"),
                11,
                palette.dark_foreground,
                true,
                false,
            );
        }
        let h = clipped(r, clipped(rows, viewport));
        hit(hits, h, generation, Action::ColumnEntry(id, i));
        if active && h.h > 0 {
            visible.push(i);
        }
    }
    if list.order.is_empty() {
        p.text(
            Rect::new(rect.x + 12, rows.y + 10, rect.w - 24, 40),
            if !list.loaded {
                "Cargando…"
            } else if !list.error.is_empty() {
                list.error
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
    let summary = if !list.error.is_empty() {
        list.error.to_owned()
    } else if !list.query.is_empty() {
        format!("{} · filtro: {}", list.order.len(), list.query)
    } else {
        format!(
            "{} elementos{}",
            list.order.len(),
            if list.loaded { "" } else { " · cargando…" }
        )
    };
    p.text(
        Rect::new(rect.x + 10, rect.y + rect.h - 27, rect.w - 20, 24),
        &summary,
        10,
        if list.error.is_empty() {
            palette.dark_foreground
        } else {
            palette.red
        },
        true,
        false,
    );
    p.unclip();
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, area: Rect) {
    m.columns.area = area;
    if m.columns.columns.is_empty() {
        m.pump_columns();
    }
    let header = 30;
    let viewport = Rect::new(area.x, area.y + header, area.w, area.h - header);
    let max = (m.columns.columns.len() as i32 * WIDTH - area.w).max(0);
    if let Some(index) = m.columns.reveal.take() {
        let left = index as i32 * WIDTH;
        let right = left + WIDTH;
        if left < m.columns.horizontal {
            m.columns.horizontal = left;
        } else if right > m.columns.horizontal + area.w {
            m.columns.horizontal = (right - area.w).max(0);
        }
    }
    m.columns.horizontal = m.columns.horizontal.clamp(0, max);
    p.text(
        Rect::new(area.x, area.y, area.w - 80, header),
        m.columns
            .notice
            .unwrap_or("COLUMNAS · ← → cambiar columna · Shift+rueda desplazar"),
        10,
        m.palette.dark_foreground,
        true,
        false,
    );
    for (offset, delta, label) in [
        (76, -WIDTH, crate::fonts::icon("", "<")),
        (36, WIDTH, crate::fonts::icon("", ">")),
    ] {
        let r = Rect::new(area.x + area.w - offset, area.y + 1, 32, 26);
        p.rounded_rect(r, 4, m.palette.lighter_background);
        p.text(
            Rect::new(r.x + 10, r.y, 20, r.h),
            label,
            12,
            m.palette.foreground,
            true,
            false,
        );
        hit(&mut m.hits, r, m.generation, Action::ColumnScroll(delta));
    }
    let active = m.columns.active;
    let available = (viewport.h - 64).max(0);
    m.content_height = m.order.len() as i32 * ROW;
    if m.loaded {
        m.scroll = m.scroll.clamp(0, (m.content_height - available).max(0));
    }
    let pending = m.columns.pending;
    for (index, c) in m.columns.columns.iter_mut().enumerate() {
        let r = Rect::new(
            area.x + index as i32 * WIDTH - m.columns.horizontal,
            viewport.y,
            WIDTH,
            viewport.h,
        );
        if index == active {
            column(
                p,
                List {
                    entries: &m.entries,
                    order: &m.order,
                    selection: &m.selection,
                    selected: m.selected,
                    query: &m.query,
                    scroll: m.scroll,
                    loaded: m.loaded,
                    error: &m.error,
                },
                &m.path,
                c.id,
                true,
                r,
                viewport,
                &m.palette,
                &m.sizes,
                m.generation,
                &mut m.hits,
                &mut m.visible,
            );
        } else {
            if c.data.loaded {
                c.data.scroll = c
                    .data
                    .scroll
                    .clamp(0, (c.data.order.len() as i32 * ROW - available).max(0));
            }
            let first = (c.data.scroll / ROW).max(0) as usize;
            let count = (available / ROW + 2) as usize;
            if let Some(worker) = m.columns.worker.as_ref() {
                // Stats belong to the actual current worker target, including refreshed ancestors.
                if pending == Some(c.id) && c.data.loaded {
                    let names = c
                        .data
                        .order
                        .iter()
                        .skip(first)
                        .take(count)
                        .copied()
                        .filter(|i| {
                            !c.data.requested.contains(i) && !c.data.entries[*i].stat_attempted
                        })
                        .map(|i| (i, c.data.entries[i].name.clone()))
                        .collect::<Vec<_>>();
                    if !names.is_empty() {
                        let ids = names.iter().map(|(i, _)| *i).collect::<Vec<_>>();
                        if worker.tx.try_send(crate::worker::Job::Stat(names)).is_ok() {
                            c.data.requested.extend(ids);
                        }
                    }
                }
            }
            column(
                p,
                List::from(&c.data),
                &c.path,
                c.id,
                false,
                r,
                viewport,
                &m.palette,
                &m.sizes,
                m.generation,
                &mut m.hits,
                &mut Vec::new(),
            );
        }
    }
    m.content_height = (m.content_height + header + 64).max(0);
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
    m.columns.reveal = Some(m.columns.active);
}
