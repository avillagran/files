//! Flat, bounded treemap of measured immediate children. No filesystem work.
use crate::{
    model::{Action, Hit, Model},
    painter::{Painter, Rect},
};
use files_core::{
    format_size,
    space_map::{self, Area, NodeKind, NodeStatus, SpaceReport},
};
use std::collections::HashMap;
#[derive(Default)]
pub struct Geometry {
    pub painted: Option<(u64, u64, &'static str, i32, i32)>,
    key: Option<(u64, u64, usize, i32, i32, bool)>,
    pub areas: Vec<Area>,
    mapped: Vec<Option<usize>>,
    excluded: Vec<usize>,
    navigation: Vec<usize>,
    membership: Vec<usize>,
    container_key: Option<(u64, u64, usize, i32, i32, bool)>,
    map_height: i32,
    pub list_rect: Rect,
    error: Option<String>,
}
impl Geometry {
    pub fn navigation_indices(&self) -> &[usize] {
        &self.navigation
    }
    pub fn contains_entry(&self, i: usize) -> bool {
        self.membership.binary_search(&i).is_ok()
    }
    pub fn current(&self, generation: u64, revision: u64, entries: usize, hidden: bool) -> bool {
        self.key.is_some_and(|k| {
            k.0 == revision && k.1 == generation && k.2 == entries && k.5 == hidden
        })
    }
    pub fn excluded_entries(&self) -> impl Iterator<Item = usize> + '_ {
        self.excluded.iter().filter_map(|&node| self.mapped[node])
    }
    pub fn max_scroll(&self) -> i32 {
        if self.list_rect.h < 26 {
            return 0;
        }
        (self.excluded.len() as i32 * 28 - self.list_rect.h).max(0)
    }
    pub fn page_rows(&self) -> i32 {
        (self.list_rect.h.saturating_add(2) / 28).max(1)
    }
    pub fn clamp_scroll(&self, scroll: i32) -> i32 {
        scroll.clamp(0, self.max_scroll())
    }
    pub fn reveal_scroll(&self, selected: Option<usize>, scroll: i32) -> i32 {
        let scroll = self.clamp_scroll(scroll);
        if self.list_rect.h < 26 {
            return scroll;
        }
        let Some(rank) = selected.and_then(|i| {
            self.excluded
                .iter()
                .position(|&node| self.mapped[node] == Some(i))
        }) else {
            return scroll;
        };
        let top = rank as i32 * 28;
        self.clamp_scroll(if top < scroll {
            top
        } else if top + 26 > scroll + self.list_rect.h {
            top + 26 - self.list_rect.h
        } else {
            scroll
        })
    }
    fn prepare(
        &mut self,
        report: &SpaceReport,
        entries: &[files_core::listing::Entry],
        generation: u64,
        revision: u64,
        size: (i32, i32),
        show_hidden: bool,
    ) -> bool {
        let key = (
            revision,
            generation,
            entries.len(),
            size.0,
            size.1,
            show_hidden,
        );
        if self.container_key == Some(key) {
            return false;
        }
        let mut rows = 0;
        loop {
            let reserve = if rows > 0 { 56 + 28 * rows } else { 26 };
            let height = (size.1 - reserve).max(24);
            self.rebuild(
                report,
                entries,
                generation,
                revision,
                (size.0, height),
                show_hidden,
            );
            self.map_height = height;
            let needed = self.excluded.len().min(3) as i32;
            if needed <= rows {
                break;
            }
            rows = needed;
        }
        self.container_key = Some(key);
        true
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    fn rebuild(
        &mut self,
        report: &SpaceReport,
        entries: &[files_core::listing::Entry],
        generation: u64,
        revision: u64,
        size: (i32, i32),
        show_hidden: bool,
    ) {
        let (w, h) = size;
        let key = (revision, generation, entries.len(), w, h, show_hidden);
        if self.key == Some(key) {
            return;
        }
        self.clear();
        self.key = Some(key);
        match space_map::layout_where(&report.nodes, w as f64, h as f64, |n| {
            show_hidden || n.name.first() != Some(&b'.')
        }) {
            Ok(areas) => self.areas = areas,
            Err(e) => self.error = Some(e),
        }
        self.mapped.resize(report.nodes.len(), None);
        let names = report
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| show_hidden || n.name.first() != Some(&b'.'))
            .map(|(i, n)| (n.name.as_slice(), i))
            .collect::<HashMap<_, _>>();
        for (i, entry) in entries.iter().enumerate() {
            if let Some(&node) = names.get(entry.name.as_encoded_bytes()) {
                self.mapped[node] = Some(i);
            }
        }
        self.excluded = report
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                (show_hidden || n.name.first() != Some(&b'.'))
                    && (n.status != NodeStatus::Complete || n.bytes.is_none_or(|b| b == 0))
            })
            .map(|(i, _)| i)
            .collect();
        for a in &self.areas {
            if a.w < 65.0 || a.h < 45.0 {
                self.excluded.push(a.node);
            }
        }
        let mut visited = vec![false; report.nodes.len()];
        for node in self
            .areas
            .iter()
            .map(|a| a.node)
            .chain(self.excluded.iter().copied())
        {
            if !visited[node] {
                visited[node] = true;
                if let Some(i) = self.mapped[node] {
                    self.navigation.push(i);
                }
            }
        }
        self.membership.extend_from_slice(&self.navigation);
        self.membership.sort_unstable();
    }
}
/// Shared keyboard/wheel/button scroll geometry; no filesystem work or new scan.
pub fn reveal(m: &mut Model) {
    m.scroll = m.space_geometry.reveal_scroll(m.selected, m.scroll);
}
pub fn scroll(m: &mut Model, rows: i32) {
    m.scroll = m
        .space_geometry
        .clamp_scroll(m.scroll.saturating_add(rows.saturating_mul(28)));
}
fn text(
    p: &mut dyn Painter,
    r: Rect,
    s: &str,
    color: files_core::theme::Color,
    size: i32,
    bold: bool,
) {
    p.text(r, s, size, color, bold, false);
}
fn button(p: &mut dyn Painter, m: &mut Model, r: Rect, label: &str, a: Action) {
    p.rounded_rect(r, 6 * m.radius, m.palette.lighter_background);
    text(
        p,
        Rect::new(r.x + 8, r.y, r.w - 16, r.h),
        label,
        m.palette.accent,
        12,
        false,
    );
    m.hits.push(Hit {
        rect: r,
        action: a,
        generation: m.generation,
    });
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, body: Rect) {
    use crate::space_map_state::View;
    m.content_height = body.h;
    let state = match m.space_map.view() {
        View::Closed => "closed",
        View::Loading => "loading",
        View::Ready(_) => "ready",
        View::Error(_) => "error",
    };
    let stamp = Some((m.space_map.revision(), m.generation, state, body.w, body.h));
    m.space_geometry.painted = stamp;
    let c = m.palette.clone();
    let (title, total) = match m.space_map.view() {
        View::Ready(report) => {
            let bytes = report
                .nodes
                .iter()
                .filter(|n| m.show_hidden || n.name.first() != Some(&b'.'))
                .filter_map(|n| n.bytes)
                .sum::<u64>();
            (
                format!(
                    "Espacio · {} visibles{}",
                    format_size(bytes),
                    if report.complete { "" } else { " · parcial" }
                ),
                format!(
                    "Total aparente {}{} · incl. ocultos",
                    if report.complete { "" } else { "≥" },
                    format_size(report.observed_bytes)
                ),
            )
        }
        View::Loading => ("Espacio · calculando…".into(), String::new()),
        View::Error(_) => ("Espacio · no disponible".into(), String::new()),
        View::Closed => (
            if m.space_requested {
                "Espacio · esperando…"
            } else {
                "Espacio · detenido"
            }
            .into(),
            String::new(),
        ),
    };
    text(
        p,
        Rect::new(body.x, body.y + 2, (body.w - 194).max(1), 28),
        &title,
        c.foreground,
        13,
        true,
    );
    button(
        p,
        m,
        Rect::new(body.x + (body.w - 186).max(0), body.y, 184, 28),
        if m.show_hidden {
            "  Ocultar ocultos"
        } else {
            "  Mostrar ocultos"
        },
        Action::ToggleHidden,
    );
    let y = body.y + 34;
    if let Some(parent) = m.path.parent().map(|p| p.to_path_buf()) {
        button(
            p,
            m,
            Rect::new(body.x, y, 82, 28),
            "  Volver",
            Action::Navigate(parent),
        );
    }
    button(
        p,
        m,
        Rect::new(body.x + 88, y, 112, 28),
        " Recalcular",
        Action::SpaceRefresh,
    );
    let controls = if !m.space_map.is_drained() {
        button(
            p,
            m,
            Rect::new(body.x + 206, y, 36, 28),
            "",
            Action::SpaceCancel,
        );
        248
    } else {
        206
    };
    text(
        p,
        Rect::new(body.x + controls, y, (body.w - controls).max(0), 28),
        &total,
        c.dark_foreground,
        10,
        false,
    );
    let status = if m.space_map.watch_gap() {
        "Vigilancia interrumpida · F5 recalcula"
    } else if m.space_map.changed() {
        "Cambios detectados · F5 recalcula"
    } else {
        "Observación · F5 o Recalcular actualiza"
    };
    text(
        p,
        Rect::new(body.x, body.y + 66, (body.w - 74).max(0), 18),
        status,
        if m.space_map.changed() || m.space_map.watch_gap() {
            c.yellow
        } else {
            c.dark_foreground
        },
        10,
        false,
    );
    text(
        p,
        Rect::new(body.x, body.y + 88, body.w, 18),
        "Subcarpetas sin vigilancia · no es una instantánea",
        c.dark_foreground,
        10,
        false,
    );
    button(
        p,
        m,
        Rect::new(body.x + (body.w - 66).max(0), body.y + 64, 66, 24),
        "Marcas",
        Action::SelectionMenu(body.x + body.w, body.y + body.h),
    );
    let map_y = body.y + 108;
    let View::Ready(report) = m.space_map.view() else {
        let message = match m.space_map.view() {
            View::Error(e) => e.as_str(),
            View::Loading => "Calculando tamaños…",
            _ => "Cálculo detenido · Actualizar para volver a calcular",
        };
        text(
            p,
            Rect::new(body.x, map_y, body.w, 50),
            message,
            c.dark_foreground,
            12,
            false,
        );
        return;
    };
    let geometry_changed = m.space_geometry.prepare(
        report,
        &m.entries,
        m.generation,
        m.space_map.revision(),
        (body.w.max(1), body.y + body.h - map_y),
        m.show_hidden,
    );
    let map = Rect::new(body.x, map_y, body.w.max(1), m.space_geometry.map_height);
    let list_top = map.y + map.h + 32;
    m.space_geometry.list_rect = Rect::new(
        body.x,
        list_top,
        body.w,
        (body.y + body.h - list_top - 24).max(0),
    );
    m.scroll = if geometry_changed {
        m.space_geometry.reveal_scroll(m.selected, m.scroll)
    } else {
        m.space_geometry.clamp_scroll(m.scroll)
    };
    m.space_geometry.painted = stamp;
    if let Some(error) = &m.space_geometry.error {
        text(p, map, error, c.red, 12, false);
    } else if m.space_geometry.areas.is_empty() {
        text(
            p,
            map,
            if report.nodes.is_empty() && report.complete {
                "Carpeta vacía · 0 B"
            } else {
                "Sin bytes completos positivos para representar"
            },
            c.dark_foreground,
            13,
            false,
        );
    }
    p.clip(map);
    let colors = [c.blue, c.cyan, c.magenta, c.green, c.yellow];
    for area in &m.space_geometry.areas {
        let node = &report.nodes[area.node];
        let x0 = area.x.round() as i32;
        let x1 = (area.x + area.w).round() as i32;
        let y0 = area.y.round() as i32;
        let y1 = (area.y + area.h).round() as i32;
        let r = Rect::new(map.x + x0, map.y + y0, x1 - x0, y1 - y0);
        if r.w <= 0 || r.h <= 0 {
            continue;
        }
        let entry = m.space_geometry.mapped[area.node];
        let selected = entry.is_some_and(|i| m.selection.contains(&i));
        let hovered = entry.is_some_and(|i| m.hovered == Some(i));
        let focused = entry.is_some_and(|i| m.selected == Some(i));
        let hash = node
            .name
            .iter()
            .fold(0usize, |v, b| v.wrapping_mul(31).wrapping_add(*b as usize));
        let fill = colors[hash % colors.len()];
        let fg = readable(fill, &c);
        let tile = Rect::new(r.x + 1, r.y + 1, (r.w - 2).max(0), (r.h - 2).max(0));
        if selected || hovered || focused {
            p.rounded_rect(tile, 7 * m.radius, fg);
        }
        let gap = if focused {
            5
        } else if selected || hovered {
            3
        } else {
            1
        };
        let inner = Rect::new(
            r.x + gap,
            r.y + gap,
            (r.w - 2 * gap).max(0),
            (r.h - 2 * gap).max(0),
        );
        p.rounded_rect(inner, 6 * m.radius, fill);
        p.clip(inner);
        if r.w >= 65 && r.h >= 45 {
            let name = files_core::name_label::bytes_label(&node.name);
            let icon = if node.kind == NodeKind::Directory {
                ""
            } else {
                ""
            };
            text(
                p,
                Rect::new(r.x + 10, r.y + 7, r.w - 20, 24),
                &format!("{icon}  {name}"),
                fg,
                13,
                true,
            );
            let size = if node.kind == NodeKind::Directory && r.w > 220 {
                format!(
                    "{} · {} {}",
                    format_size(node.bytes.unwrap_or(0)),
                    node.files,
                    if node.files == 1 {
                        "archivo"
                    } else {
                        "archivos"
                    }
                )
            } else {
                format_size(node.bytes.unwrap_or(0))
            };
            text(
                p,
                Rect::new(r.x + 10, (r.y + r.h - 32).max(r.y + 30), r.w - 20, 24),
                &size,
                fg,
                12,
                false,
            );
        }
        p.unclip();
        if let Some(i) = entry {
            m.hits.push(Hit {
                rect: r,
                action: Action::Entry(i),
                generation: m.generation,
            });
            m.visible.push(i);
        }
    }
    p.unclip();
    let list_y = map.y + map.h + 8;
    if !m.space_geometry.excluded.is_empty() {
        let overflow = m.space_geometry.max_scroll() > 0;
        text(
            p,
            Rect::new(
                body.x,
                list_y,
                (body.w - if overflow { 68 } else { 0 }).max(0),
                22,
            ),
            &format!(
                "Sin área/etiqueta · {} · rueda / ↑↓",
                m.space_geometry.excluded.len()
            ),
            c.dark_foreground,
            10,
            false,
        );
        if overflow {
            for (offset, direction, label) in [(64, -1, "↑"), (30, 1, "↓")] {
                let rect = Rect::new(body.x + body.w - offset, list_y, 28, 22);
                p.rounded_rect(rect, 4, c.lighter_background);
                text(p, rect, label, c.foreground, 12, false);
                m.hits.push(Hit {
                    rect,
                    action: Action::SpaceListScroll(direction * m.space_geometry.page_rows()),
                    generation: m.generation,
                });
            }
        }
        let clip = m.space_geometry.list_rect;
        let list_top = clip.y;
        let list_h = clip.h;
        let scroll = m.scroll;
        m.content_height = body.h + m.space_geometry.max_scroll();
        p.clip(clip);
        for (rank, &node) in m.space_geometry.excluded.iter().enumerate() {
            let y = list_top + rank as i32 * 28 - scroll;
            if y < list_top || y + 26 > list_top + list_h {
                continue;
            }
            let n = &report.nodes[node];
            let state = match (n.status, n.bytes) {
                (NodeStatus::Complete, Some(0)) => "0 B · sin área".into(),
                (NodeStatus::Complete, Some(b)) => {
                    format!("{} · etiqueta en lista", format_size(b))
                }
                (NodeStatus::Partial, _) => format!("PARCIAL ≥{}", format_size(n.observed_bytes)),
                (NodeStatus::Error, _) => "ERROR · tamaño desconocido".into(),
                _ => "Tamaño desconocido".into(),
            };
            let row = Rect::new(body.x, y, body.w, 26);
            let index = m.space_geometry.mapped[node];
            if index.is_some_and(|i| m.selection.contains(&i)) {
                p.rect(row, c.selection);
            }
            if index.is_some_and(|i| m.selected == Some(i)) {
                p.rect(Rect::new(row.x, row.y + 2, 3, row.h - 4), c.foreground);
            }
            text(
                p,
                Rect::new(body.x + 6, y, body.w - 12, 26),
                &format!("{} · {state}", files_core::name_label::bytes_label(&n.name)),
                c.foreground,
                11,
                false,
            );
            if let Some(i) = index {
                m.hits.push(Hit {
                    rect: row,
                    action: Action::Entry(i),
                    generation: m.generation,
                });
                m.visible.push(i);
            }
        }
        p.unclip();
    }
    let footer = report.reason.as_deref().unwrap_or(if report.complete {
        "Las carpetas incluyen descendientes ocultos"
    } else {
        "Recorrido parcial: total de áreas excluye tamaños desconocidos"
    });
    text(
        p,
        Rect::new(body.x, body.y + body.h - 22, body.w, 22),
        footer,
        if report.complete {
            c.dark_foreground
        } else {
            c.yellow
        },
        10,
        false,
    );
}
fn luminance(c: files_core::theme::Color) -> f64 {
    let channel = |v: u8| {
        let n = v as f64 / 255.;
        if n <= 0.04045 {
            n / 12.92
        } else {
            ((n + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(c.0) + 0.7152 * channel(c.1) + 0.0722 * channel(c.2)
}
fn contrast(a: files_core::theme::Color, b: files_core::theme::Color) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
fn readable(
    fill: files_core::theme::Color,
    p: &files_core::theme::Palette,
) -> files_core::theme::Color {
    let best = if contrast(fill, p.darker_background) > contrast(fill, p.bright_foreground) {
        p.darker_background
    } else {
        p.bright_foreground
    };
    if contrast(fill, best) >= 4.5 {
        best
    } else {
        let black = files_core::theme::Color(0, 0, 0);
        let white = files_core::theme::Color(255, 255, 255);
        if contrast(fill, black) > contrast(fill, white) {
            black
        } else {
            white
        }
    }
}

pub fn inspector_details(m: &Model, index: usize) -> Option<String> {
    let crate::space_map_state::View::Ready(report) = m.space_map.view() else {
        return None;
    };
    if m.space_map.path() != Some(m.path.as_path()) {
        return None;
    }
    let entry = m.entries.get(index)?;
    if !files_core::visibility::accepts(&entry.name, m.show_hidden) {
        return None;
    }
    let n = report
        .nodes
        .iter()
        .find(|n| n.name == entry.name.as_encoded_bytes())?;
    Some(match (n.status, n.bytes) {
        (NodeStatus::Complete, Some(bytes)) => format!(
            "Aparente {}\n{} archivos · {} carpetas\n{}",
            format_size(bytes),
            n.files,
            n.directories,
            if matches!(n.kind, NodeKind::Symlink | NodeKind::Other) {
                "Enlace/especial excluido"
            } else {
                "Medición completa, no snapshot"
            }
        ),
        _ => format!(
            "{:?} · ≥{} observado\n{}",
            n.status,
            format_size(n.observed_bytes),
            n.error.as_deref().unwrap_or("Tamaño completo desconocido")
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use files_core::{
        listing::{Entry, Kind},
        space_map::SpaceNode,
    };
    fn fixture() -> (SpaceReport, Vec<Entry>) {
        let nodes = [("photos", 8), ("docs", 4), ("video", 2), ("empty", 0)]
            .into_iter()
            .map(|(n, b)| SpaceNode {
                name: n.as_bytes().into(),
                kind: NodeKind::File,
                bytes: Some(b),
                observed_bytes: b,
                files: 1,
                directories: 0,
                status: NodeStatus::Complete,
                error: None,
            })
            .collect::<Vec<_>>();
        let entries = nodes
            .iter()
            .rev()
            .map(|n| Entry {
                name: std::ffi::OsString::from(std::str::from_utf8(&n.name).unwrap()),
                kind: Kind::File,
                metadata: None,
                stat_attempted: false,
            })
            .collect();
        (
            SpaceReport {
                nodes,
                total_bytes: 14,
                observed_bytes: 14,
                visited: 4,
                complete: true,
                reason: None,
                omitted_children: Some(0),
            },
            entries,
        )
    }
    fn selection_model() -> Model {
        let (mut report, mut entries) = fixture();
        report.complete = false;
        report.omitted_children = Some(1);
        entries.push(Entry {
            name: "outside-report".into(),
            kind: Kind::File,
            metadata: None,
            stat_attempted: false,
        });
        let mut m = Model::new(std::env::temp_dir());
        m.view = crate::model::View::SpaceMap;
        m.entries = entries;
        m.order = (0..m.entries.len()).collect();
        m.loaded = true;
        m.query = "not-a-map-filter".into();
        m.space_map = crate::space_map_state::State::ready_for_test(m.path.clone(), report.clone());
        m.space_geometry.prepare(
            &report,
            &m.entries,
            m.generation,
            m.space_map.revision(),
            (356, 198),
            false,
        );
        m
    }
    #[test]
    fn map_bulk_marks_use_projection_and_preserve_ignored_query() {
        use crate::selection_tools::Command;
        let mut m = selection_model();
        m.selection_command(Command::All);
        assert_eq!(m.selection.len(), 4);
        assert!(!m.selection.contains(&4));
        assert_eq!(m.query, "not-a-map-filter");
        m.selection_command(Command::Invert);
        assert!(m.selection.is_empty());
        assert_eq!(m.selected, None);
        assert!(m.preserve_empty_selection);
        m.selection.insert(4);
        m.selection_command(Command::Invert);
        assert_eq!(m.selection.len(), 4);
        assert!(!m.selection.contains(&4));
        let marks = m.selection.clone();
        m.selection_command(Command::Filtered);
        assert_eq!(m.selection, marks);
        assert_eq!(m.query, "not-a-map-filter");
        assert!(m.error.contains("no aplica el filtro"));
    }
    #[test]
    fn map_stale_projection_blocks_marking_but_clear_survives_close() {
        use crate::selection_tools::Command;
        let mut m = selection_model();
        m.selection.clear();
        m.show_hidden = true;
        m.selection_command(Command::All);
        assert!(m.selection.is_empty());
        assert!(m.error.contains("proyección vigente"));
        m.show_hidden = false;
        m.generation += 1;
        m.mark_and_advance();
        assert!(m.selection.is_empty());
        m.selection.insert(0);
        m.space_map.close();
        m.space_geometry.clear();
        m.selection_command(Command::Clear);
        assert!(m.selection.is_empty());
        assert_eq!(m.selected, None);
    }
    #[test]
    fn map_menu_and_palette_reject_hidden_projection_changes() {
        use crate::selection_tools::Command;
        let mut m = selection_model();
        m.selection.clear();
        crate::context_menu::open_selection(&mut m, 440, 440);
        let menu = m.context_menu.as_ref().unwrap();
        assert!(
            menu.items
                .iter()
                .any(|i| i.label.contains("Filtro no aplicado") && i.action.is_none())
        );
        let all = menu
            .items
            .iter()
            .position(|i| matches!(i.action, Some(Action::Selection(Command::All))))
            .unwrap();
        m.show_hidden = true;
        crate::context_menu::activate(&mut m, all);
        assert!(m.selection.is_empty());
        assert!(m.error.contains("mapa cambió"));
        m.show_hidden = false;
        m.open_commands();
        let all = crate::command_palette::ITEMS
            .iter()
            .position(|s| s.id == "select-all")
            .unwrap();
        assert!(m.commands.as_ref().unwrap().reason(&m, all).is_none());
        m.show_hidden = true;
        assert!(
            m.commands
                .as_ref()
                .unwrap()
                .reason(&m, all)
                .unwrap()
                .contains("mapa cambió")
        );
        m.commands = None;
    }
    #[test]
    fn map_insert_advances_in_projection_order_and_stops_at_last() {
        let mut m = selection_model();
        let order = m.space_geometry.navigation_indices().to_vec();
        m.selection.clear();
        m.selected = Some(order[0]);
        m.mark_and_advance();
        assert!(m.selection.contains(&order[0]));
        assert_eq!(m.selected, Some(order[1]));
        m.selected = order.last().copied();
        m.mark_and_advance();
        assert_eq!(m.selected, order.last().copied());
        assert!(m.selection.contains(order.last().unwrap()));
    }
    #[test]
    fn projection_keeps_byte_ratios_entry_identity_and_zero_out_of_areas() {
        let (report, entries) = fixture();
        let mut geometry = Geometry::default();
        for (w, h) in [(800, 400), (320, 700)] {
            geometry.rebuild(&report, &entries, 4, 7, (w, h), false);
            assert_eq!(geometry.areas.len(), 3);
            assert_eq!(geometry.excluded, vec![3]);
            for a in &geometry.areas {
                let expected =
                    w as f64 * h as f64 * report.nodes[a.node].bytes.unwrap() as f64 / 14.;
                assert!((a.w * a.h - expected).abs() < 1e-6);
                assert_eq!(geometry.mapped[a.node], Some(3 - a.node));
            }
            assert_eq!(geometry.navigation_indices().len(), 4);
        }
        geometry.clear();
        assert!(
            geometry.areas.is_empty() && geometry.mapped.is_empty() && geometry.painted.is_none()
        );
    }
    #[test]
    fn refresh_generation_rebuilds_mapping_without_stale_entry_targets() {
        let (report, mut entries) = fixture();
        let mut geometry = Geometry::default();
        geometry.rebuild(&report, &entries, 1, 1, (800, 400), false);
        entries.reverse();
        geometry.rebuild(&report, &entries, 2, 1, (800, 400), false);
        assert_eq!(geometry.mapped, vec![Some(0), Some(1), Some(2), Some(3)]);
        entries.clear();
        geometry.rebuild(&report, &entries, 3, 1, (800, 400), false);
        assert!(geometry.navigation_indices().is_empty());
    }
    #[test]
    fn hidden_projection_has_no_targets_but_preserves_recursive_totals() {
        let (mut report, mut entries) = fixture();
        report.nodes[0].name = b".photos".to_vec();
        entries[3].name = ".photos".into();
        let mut geometry = Geometry::default();
        geometry.rebuild(&report, &entries, 1, 1, (600, 300), false);
        assert_eq!(geometry.areas.len(), 2);
        assert_eq!(geometry.mapped[0], None);
        assert!(!geometry.navigation_indices().contains(&3));
        assert_eq!(report.total_bytes, 14);
        for a in &geometry.areas {
            assert!(
                (a.w * a.h - 600. * 300. * report.nodes[a.node].bytes.unwrap() as f64 / 6.).abs()
                    < 1e-7
            );
        }
        geometry.rebuild(&report, &entries, 1, 1, (600, 300), true);
        assert_eq!(geometry.areas.len(), 3);
        assert!(geometry.navigation_indices().contains(&3));
    }
    #[test]
    fn label_contrast_remains_readable_on_both_light_and_dark_fills() {
        let palette = files_core::theme::Palette::default();
        for r in (0..=255).step_by(51) {
            for g in (0..=255).step_by(51) {
                for b in (0..=255).step_by(51) {
                    let fill = files_core::theme::Color(r, g, b);
                    assert!(contrast(fill, readable(fill, &palette)) >= 4.5);
                }
            }
        }
    }
    fn extreme_fixture() -> (SpaceReport, Vec<Entry>) {
        let (mut report, _) = fixture();
        report.nodes.clear();
        for i in 0..14 {
            let bytes = if i == 0 { 1u64 << 40 } else { 1 };
            report.nodes.push(SpaceNode {
                name: format!("entry-{i:02}").into_bytes(),
                kind: NodeKind::File,
                bytes: Some(bytes),
                observed_bytes: bytes,
                files: 1,
                directories: 0,
                status: NodeStatus::Complete,
                error: None,
            });
        }
        report.total_bytes = (1u64 << 40) + 13;
        report.observed_bytes = report.total_bytes;
        report.visited = 14;
        let entries = report
            .nodes
            .iter()
            .rev()
            .map(|n| Entry {
                name: std::ffi::OsString::from(std::str::from_utf8(&n.name).unwrap()),
                kind: Kind::File,
                metadata: None,
                stat_attempted: false,
            })
            .collect();
        (report, entries)
    }
    #[test]
    fn positive_only_tiny_blocks_get_rows_without_inflating_area_and_cache_navigation() {
        let (report, entries) = extreme_fixture();
        let mut g = Geometry::default();
        assert!(g.prepare(&report, &entries, 1, 2, (356, 198), false));
        assert_eq!(g.excluded.len(), 13);
        assert_eq!(g.navigation_indices().len(), 14);
        g.list_rect = Rect::new(68, 0, 356, 84);
        assert!(g.map_height < 198 - 26);
        assert_eq!(g.max_scroll(), 13 * 28 - 84);
        let sum = g.areas.iter().map(|a| a.w * a.h).sum::<f64>();
        assert!((sum - 356. * f64::from(g.map_height)).abs() < 1e-6);
        for area in &g.areas {
            let fraction = area.w * area.h / sum;
            assert!(
                (fraction
                    - report.nodes[area.node].bytes.unwrap() as f64 / report.total_bytes as f64)
                    .abs()
                    < 1e-12
            );
        }
        let cached = g.navigation_indices().as_ptr();
        assert!(!g.prepare(&report, &entries, 1, 2, (356, 198), false));
        assert_eq!(cached, g.navigation_indices().as_ptr());
        let last = g.excluded_entries().last().unwrap();
        let scroll = g.reveal_scroll(Some(last), 0);
        let rank = g
            .excluded
            .iter()
            .position(|&n| g.mapped[n] == Some(last))
            .unwrap() as i32;
        assert!(rank * 28 >= scroll && rank * 28 + 26 <= scroll + g.list_rect.h);
        assert_eq!(g.clamp_scroll(i32::MAX), g.max_scroll());
        assert_eq!(g.clamp_scroll(i32::MIN), 0);
        g.list_rect.h = 26;
        assert_eq!(g.reveal_scroll(Some(last), 0), rank * 28);
        g.list_rect.h = 25;
        assert_eq!(g.reveal_scroll(Some(last), i32::MAX), 0);
        // The MC strip leaves a 24px map: even the large tile needs a named row.
        assert!(g.prepare(&report, &entries, 1, 2, (356, 162), false));
        assert_eq!(g.map_height, 24);
        assert_eq!(g.excluded.len(), 14);
        g.list_rect.h = 82;
        assert_eq!(g.page_rows(), 3);
    }
    #[test]
    fn keyboard_range_focus_and_scroll_follow_map_order_with_shuffled_entry_indices() {
        use fltk::enums::Key;
        let (report, entries) = extreme_fixture();
        let mut m = Model::new(std::env::temp_dir());
        m.view = crate::model::View::SpaceMap;
        m.entries = entries;
        m.order = (0..m.entries.len()).collect();
        m.loaded = true;
        m.selected = None;
        m.anchor = None;
        m.selection.clear();
        m.space_geometry.prepare(
            &report,
            &m.entries,
            m.generation,
            m.space_map.revision(),
            (356, 198),
            false,
        );
        m.space_geometry.list_rect = Rect::new(68, 0, 356, 84);
        let order = m.space_geometry.navigation_indices().to_vec();
        assert_ne!(order, m.order);
        crate::keyboard_nav::key(&mut m, Key::End, false, false, 440, 440);
        assert_eq!(m.selected, order.last().copied());
        assert!(m.scroll > 0);
        crate::keyboard_nav::key(&mut m, Key::Home, false, false, 440, 440);
        assert_eq!(m.selection.len(), 1);
        crate::keyboard_nav::key(&mut m, Key::End, false, true, 440, 440);
        assert_eq!(m.selection, order.iter().copied().collect());
        let marks = m.selection.clone();
        crate::keyboard_nav::key(&mut m, Key::Home, true, false, 440, 440);
        assert_eq!(m.selected, Some(order[0]));
        assert_eq!(m.selection, marks);
        m.select_entry(order[3], false, true);
        assert_eq!(m.selection, order[..=3].iter().copied().collect());
        scroll(&mut m, i32::MAX);
        assert_eq!(m.scroll, m.space_geometry.max_scroll());
        scroll(&mut m, i32::MIN);
        assert_eq!(m.scroll, 0);
        m.space_geometry.clear();
        let before = m.selected;
        crate::keyboard_nav::key(&mut m, Key::End, false, false, 440, 440);
        assert_eq!(m.selected, before);
        // Cancel/clear may leave old paint hits with the same listing generation.
        // Neither double-click nor Enter may open an old, unmapped focus then.
        let i = before.unwrap();
        m.entries[i].kind = Kind::Directory;
        m.hits.push(Hit {
            rect: Rect::new(0, 0, 10, 10),
            action: Action::Entry(i),
            generation: m.generation,
        });
        let path = m.path.clone();
        let generation = m.generation;
        m.pointer_down(5, 5, false, false, true);
        m.open_selected();
        assert_eq!(m.path, path);
        assert_eq!(m.generation, generation);
    }
}
