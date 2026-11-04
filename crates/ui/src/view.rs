use crate::{
    model::{Action, Hit, Model, View},
    painter::{Painter, Rect},
};
use files_core::{
    format_size, mime, relative_date, size_parts,
    theme::{Color, Corners, WindowMode},
};
use std::{path::PathBuf, time::SystemTime};
fn r(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect::new(x, y, w, h)
}
fn hit(m: &mut Model, rect: Rect, action: Action) {
    m.hits.push(Hit {
        rect,
        action,
        generation: m.generation,
    });
}
fn button(p: &mut dyn Painter, m: &mut Model, rect: Rect, label: &str, on: bool, action: Action) {
    let unavailable = crate::copy_review::reason(m, &action)
        .or_else(|| crate::map_actions::reason(m, &action))
        .is_some();
    let c = &m.palette;
    p.rounded_rect(
        rect,
        7 * m.radius,
        if on {
            c.lighter_background
        } else {
            c.darker_background
        },
    );
    p.text(
        r(rect.x + 10, rect.y, rect.w - 20, rect.h),
        label,
        12,
        if unavailable {
            c.muted
        } else if on {
            c.accent
        } else {
            c.dark_foreground
        },
        false,
        false,
    );
    hit(m, rect, action);
}
fn badge_color(m: &Model, path: &std::path::Path) -> Color {
    let mime = mime::mime(path);
    let c = &m.palette;
    if mime.starts_with("image/") {
        c.magenta
    } else if mime.contains("zip")
        || mime.contains("compressed")
        || mime.contains("tar")
        || mime.contains("gzip")
    {
        c.yellow
    } else if mime.contains("shell") || mime.contains("python") || mime.contains("rust") {
        c.green
    } else if mime.starts_with("text/") || mime.contains("json") || mime.contains("toml") {
        c.cyan
    } else {
        c.blue
    }
}
fn folder(p: &mut dyn Painter, x: i32, y: i32, c: Color) {
    p.text(
        r(x, y - 2, 34, 32),
        crate::fonts::icon("", "DIR"),
        25,
        c,
        true,
        false,
    );
}

pub fn tip_rect(w: i32, h: i32) -> Rect {
    r(208, h - 31, (w - 708).max(0), 30)
}
pub fn tips_available(m: &Model, w: i32) -> bool {
    w >= 788
        && m.bookmarks.panel.is_none()
        && m.loaded
        && !m.closing
        && m.bookmarks.status().is_none()
        && m.folder_views.status().is_none()
        && m.error.is_empty()
        && m.session.message().is_none()
        && m.config_error.is_none()
        && m.saves.last_error().is_none()
        && !m.selection_refreshing()
        && !m.mutations.busy
        && !m.mutation_applied
        && !m.copy_busy()
        && !m.sizes.has_work()
        && m.sizes.errors() == 0
        && !m.filter_edit.active
        && m.context_menu.is_none()
        && m.help.is_none()
        && m.properties_dialog.is_none()
        && m.rename_dialog.is_none()
}
/// Preserve the marquee's full measured width, but indicate clipping during
/// its initial pause rather than ending a visible phrase mid-word.
fn tip_at_rest(p: &mut dyn Painter, text: &str, available: i32) -> String {
    let width = available.max(0) as f32;
    if p.text_width(text, 11, true) <= width {
        return text.to_owned();
    }
    let ellipsis = "…";
    if p.text_width(ellipsis, 11, true) > width {
        return String::new();
    }
    let mut best = 0;
    for (at, ch) in text.char_indices() {
        let end = at + ch.len_utf8();
        if p.text_width(&format!("{}…", &text[..end]), 11, true) > width {
            break;
        }
        best = end;
    }
    format!("{}…", text[..best].trim_end())
}
pub fn status(p: &mut dyn Painter, m: &mut Model, w: i32, h: i32) {
    let c = &m.palette;
    p.rect(r(0, h - 32, w, 32), c.darker_background);
    p.rect(r(0, h - 32, w, 1), c.lighter_background);
    let fs = m
        .mounts
        .iter()
        .filter(|d| m.path.starts_with(&d.path))
        .max_by_key(|d| d.path.components().count())
        .map(|d| d.fs.as_str())
        .unwrap_or("FS");
    let save_error = m
        .saves
        .last_error()
        .map(|e| format!("Sin guardar {}: {}", e.directory.display(), e.message));
    let mut size_status = false;
    let left = if let Some(e) = &m.config_error {
        e.clone()
    } else if let Some(e) = &save_error {
        e.clone()
    } else if let Some(error) = files_core::name_filter::Filter::new(&m.query).error() {
        error.into()
    } else if !m.error.is_empty() {
        m.error.clone()
    } else if let Some(message) = m.launch_notice {
        message.into()
    } else if let Some(status) = m.folder_views.status() {
        status.into()
    } else if let Some(status) = m.bookmarks.status() {
        status.into()
    } else if let Some(sizes) = m.sizes.summary() {
        size_status = true;
        if w < 640 {
            m.sizes.compact_summary().unwrap_or(sizes)
        } else {
            sizes
        }
    } else if m.filter_edit.active {
        "Enter: lista · Esc: limpiar".into()
    } else if let Some(message) = m.session.message() {
        message.into()
    } else if m.selection_refreshing() {
        "Actualizando · selección bloqueada hasta terminar".into()
    } else if !m.listing_sync.notice.is_empty() {
        m.listing_sync.notice.clone()
    } else {
        format!(
            "{} · {}{} elementos{}",
            if m.selection.is_empty() && m.selected.is_none() {
                "Sin selección".into()
            } else {
                format!(
                    "{} {}",
                    m.selection.len(),
                    if m.selection.len() == 1 {
                        "marcado"
                    } else {
                        "marcados"
                    }
                )
            },
            m.order.len(),
            if m.query.is_empty() {
                String::new()
            } else {
                format!(" de {}", m.entries.len())
            },
            if m.loaded { "" } else { " · cargando…" }
        )
    };
    let tips = tips_available(m, w);
    p.text(
        r(
            16,
            h - 31,
            if tips {
                180
            } else if w < 640 || size_status {
                (w - 212).max(100)
            } else {
                (w - 520).max(100)
            },
            30,
        ),
        &left,
        12,
        if m.error.is_empty()
            && save_error.is_none()
            && m.config_error.is_none()
            && m.bookmarks.error.is_empty()
            && m.sizes.errors() == 0
        {
            c.dark_foreground
        } else {
            c.red
        },
        false,
        false,
    );
    let tip = tip_rect(w, h);
    if tips {
        let label = m.status_tips.text();
        let width = p.text_width(label, 11, true);
        m.status_tips.configure(tip.w, f64::from(width));
        let resting = if m.status_tips.offset() == 0 && width > tip.w as f32 {
            Some(tip_at_rest(p, label, tip.w))
        } else {
            None
        };
        p.clip(tip);
        p.text(
            r(
                tip.x - m.status_tips.offset(),
                tip.y,
                (width.ceil() as i32 + 2).max(tip.w),
                tip.h,
            ),
            resting.as_deref().unwrap_or(label),
            11,
            c.dark_foreground,
            true,
            false,
        );
        p.unclip();
    } else {
        m.status_tips.configure(0, 0.);
    }
    if w >= 640 && !size_status {
        let free = m.free.map(format_size).unwrap_or_else(|| "—".into());
        p.text(
            r((w - 480).max(150), h - 31, 280, 30),
            &format!("{fs} · {free} libres"),
            12,
            c.dark_foreground,
            false,
            true,
        );
    }
    let pss = m
        .pss
        .map(|k| format!("PSS {:.1} MiB", k as f64 / 1024.))
        .unwrap_or_else(|| "PSS —".into());
    p.text(r(w - 180, h - 31, 160, 30), &pss, 12, c.green, true, true);
}
pub fn omni_right(w: i32) -> i32 {
    w - if w >= 1100 {
        776
    } else if w >= 800 {
        520
    } else {
        144
    }
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, w: i32, h: i32) {
    m.listing_sync.width = w;
    if m.search_reveal {
        m.search_reveal = false;
        if m.dual.enabled {
            crate::dual_view::reveal(m, h);
        } else if m.view == View::Columns {
            crate::columns_view::reveal(m, h);
        } else {
            reveal_selection(m, w, h);
        }
    }
    m.pump_media_source();
    m.thumbnails.painted = 0;
    m.hits.clear();
    m.visible.clear();
    m.visible_generation = m.generation;
    let c = m.palette.clone();
    let rad = m.radius;
    p.rect(r(0, 0, w, h), c.background);
    p.rect(r(0, 0, w, 58), c.background);
    for (x, label, forward, enabled) in [
        (
            12,
            crate::fonts::icon("", "<"),
            false,
            m.history.can_back(),
        ),
        (
            48,
            crate::fonts::icon("", ">"),
            true,
            m.history.can_forward(),
        ),
    ] {
        let area = r(x, 13, 30, 32);
        p.text(
            area,
            label,
            16,
            if enabled { c.foreground } else { c.muted },
            true,
            false,
        );
        hit(
            m,
            area,
            if enabled {
                Action::History(forward)
            } else {
                Action::None
            },
        );
    }
    let omni = if m.editing {
        r(88, 10, (w - 192).max(1), 38)
    } else {
        r(
            88,
            10,
            (omni_right(w) - 152 - if m.query.is_empty() { 0 } else { 30 }).max(1),
            38,
        )
    };
    p.rounded_rect(omni, 10 * rad, c.darker_background);
    hit(
        m,
        omni,
        if m.filter_edit.active || !m.query.is_empty() {
            Action::Filter
        } else {
            Action::Omni
        },
    );
    p.clip(r(omni.x + 8, omni.y + 2, omni.w - 16, omni.h - 4));
    if m.editing {
        crate::path_edit::paint(p, m, r(omni.x + 8, omni.y + 2, omni.w - 16, omni.h - 4));
    } else if m.filter_edit.active || !m.query.is_empty() {
        crate::filter_edit::paint(p, m, r(omni.x + 8, omni.y + 2, omni.w - 16, omni.h - 4));
    } else {
        let mut crumbs = Vec::new();
        let home = files_core::theme::home();
        let mut path = if m.path.starts_with(&home) {
            home.clone()
        } else {
            PathBuf::from("/")
        };
        crumbs.push((
            if path == home { "~".into() } else { "/".into() },
            path.clone(),
        ));
        if let Ok(rest) = m.path.strip_prefix(&path) {
            for part in rest.components() {
                path.push(part);
                crumbs.push((
                    files_core::name_label::label(part.as_os_str()).into_owned(),
                    path.clone(),
                ));
            }
        }
        // Mantener visibles los últimos segmentos de una ruta larga.
        let mut widths = crumbs
            .iter()
            .map(|(s, _)| (s.chars().count() as i32 * 7 + 20).min(180))
            .collect::<Vec<_>>();
        while widths.iter().sum::<i32>() + (widths.len() as i32 - 1) * 14 > omni.w - 30
            && crumbs.len() > 1
        {
            crumbs.remove(0);
            widths.remove(0);
        }
        let mut x = omni.x + 12;
        for (i, ((label, path), cw)) in crumbs.iter().zip(&widths).enumerate() {
            let current = i == crumbs.len() - 1;
            p.rounded_rect(
                r(x, 17, *cw, 24),
                7 * rad,
                if current {
                    c.accent
                } else {
                    c.lighter_background
                },
            );
            p.text(
                r(x + 8, 17, *cw - 16, 24),
                label,
                12,
                if current {
                    c.darker_background
                } else {
                    c.foreground
                },
                false,
                false,
            );
            // Breadcrumbs are clipped visually; their hit target must end at
            // that same clip, leaving the free header gap available for move.
            let visible_width = (*cw).min(omni.x + omni.w - 8 - x).max(0);
            if visible_width > 0 {
                hit(
                    m,
                    r(x, 17, visible_width, 24),
                    Action::Navigate(path.clone()),
                );
            }
            x += cw + 14;
        }
        if x + 90 < omni.x + omni.w {
            p.text(
                r(x + 4, 11, omni.x + omni.w - x - 8, 36),
                "escribe para filtrar",
                12,
                c.dark_foreground,
                false,
                false,
            );
        }
    }
    p.unclip();
    if m.editing {
        button(
            p,
            m,
            r(w - 84, 10, 36, 36),
            crate::fonts::icon("", "OK"),
            false,
            Action::PathCommit,
        );
        button(
            p,
            m,
            r(w - 44, 10, 36, 36),
            crate::fonts::icon("", "X"),
            false,
            Action::PathCancel,
        );
    } else {
        if !m.query.is_empty() {
            let path_button = r(omni_right(w) - 90, 13, 28, 32);
            p.text(
                path_button,
                crate::fonts::icon("", "L"),
                16,
                c.accent,
                true,
                false,
            );
            hit(m, path_button, Action::Omni);
        }
        let filter_button = r(omni_right(w) - 60, 13, 28, 32);
        p.text(
            filter_button,
            crate::fonts::icon("", "?"),
            16,
            c.accent,
            true,
            false,
        );
        hit(m, filter_button, Action::Filter);
        let clear_button = r(omni_right(w) - 30, 13, 28, 32);
        p.text(
            clear_button,
            if m.query.is_empty() {
                crate::fonts::icon("", "L")
            } else {
                crate::fonts::icon("", "x")
            },
            16,
            c.accent,
            true,
            false,
        );
        hit(
            m,
            clear_button,
            if m.query.is_empty() {
                Action::Omni
            } else {
                Action::ClearFilter
            },
        );
        if w >= 800 {
            let wide = w >= 1100;
            let mut x = omni_right(w) + 14;
            for (width, label, on, action) in [
                (
                    if wide { 92 } else { 40 },
                    if wide {
                        crate::fonts::icon("  Grupos", "Grupos")
                    } else {
                        crate::fonts::icon("", "G")
                    },
                    m.view == View::Groups,
                    Action::View(View::Groups),
                ),
                (
                    if wide { 78 } else { 40 },
                    if wide {
                        crate::fonts::icon("  Lista", "Lista")
                    } else {
                        crate::fonts::icon("", "L")
                    },
                    m.view == View::List,
                    Action::View(View::List),
                ),
                (
                    if wide { 92 } else { 40 },
                    if wide {
                        crate::fonts::icon("  Grilla", "Grilla")
                    } else {
                        crate::fonts::icon("", "G")
                    },
                    m.view == View::Grid,
                    Action::View(View::Grid),
                ),
                (
                    if wide { 110 } else { 104 },
                    crate::fonts::icon("  Espacio", "Espacio"),
                    m.view == View::SpaceMap,
                    Action::View(View::SpaceMap),
                ),
                (
                    if wide { 112 } else { 104 },
                    crate::fonts::icon("  Columnas", "Columnas"),
                    m.view == View::Columns,
                    Action::View(View::Columns),
                ),
                (
                    40,
                    if m.dual.enabled {
                        crate::fonts::icon("", "MC")
                    } else {
                        crate::fonts::icon("", "I")
                    },
                    m.inspector || m.dual.enabled,
                    Action::Inspector,
                ),
                (
                    40,
                    crate::fonts::icon("", "P"),
                    m.settings,
                    Action::Settings,
                ),
                (
                    40,
                    crate::fonts::icon("", "A"),
                    m.actions_menu,
                    Action::Actions,
                ),
            ] {
                button(p, m, r(x, 10, width, 36), label, on, action);
                x += width + 4;
            }
        } else {
            for (x, label, action) in [
                (
                    w - 140,
                    crate::fonts::icon("", "V"),
                    Action::ViewMenu(w, h),
                ),
                (w - 96, crate::fonts::icon("", "P"), Action::Settings),
                (w - 52, crate::fonts::icon("", "A"), Action::Actions),
            ] {
                button(p, m, r(x, 10, 40, 36), label, false, action);
            }
        }
    }
    let rail = if m.rail { 170 } else { 52 };
    p.rect(
        r(
            0,
            90,
            rail,
            crate::operation_bar::browser_height(m, h) - 122,
        ),
        c.darker_background,
    );
    p.rect(
        r(
            rail - 1,
            90,
            1,
            crate::operation_bar::browser_height(m, h) - 122,
        ),
        c.lighter_background,
    );
    let trash_y = crate::operation_bar::browser_height(m, h) - 72;
    p.clip(r(0, 90, rail - 1, (trash_y - 8 - 90).max(0)));
    let rail_hits_start = m.hits.len();
    let star = r(8, 98, rail - 16, 36);
    p.text(
        r(18, 98, 30, 36),
        crate::fonts::icon("", "*"),
        20,
        c.accent,
        true,
        false,
    );
    if m.rail {
        p.text(
            r(48, 98, 110, 36),
            "Favoritos",
            12,
            c.dark_foreground,
            false,
            false,
        );
    }
    hit(m, star, Action::Bookmarks);
    let mut y = 138;
    for (i, place) in m.places.clone().iter().enumerate() {
        let on = m.path == place.path;
        let rect = r(8, y, rail - 16, 36);
        if on {
            p.rounded_rect(rect, 8 * rad, c.lighter_background);
        }
        rail_icon(
            p,
            18,
            y + 10,
            i,
            if on { c.accent } else { c.dark_foreground },
        );
        if m.rail {
            p.text(
                r(48, y, 110, 36),
                &place.label,
                12,
                if on { c.foreground } else { c.dark_foreground },
                false,
                false,
            );
        }
        hit(m, rect, Action::Navigate(place.path.clone()));
        y += 40;
    }
    // Bookmarks (Nautilus sidebar): up to eight, then devices.
    let marks: Vec<(std::path::PathBuf, String)> = m
        .bookmarks
        .document
        .rows
        .iter()
        .take(8)
        .map(|e| (e.path.clone(), e.label.clone()))
        .collect();
    if !marks.is_empty() {
        p.rect(r(14, y + 6, rail - 28, 1), c.lighter_background);
        y += 18;
    }
    for (path, label) in marks {
        let on = m.path == path;
        let rect = r(8, y, rail - 16, 36);
        if on {
            p.rounded_rect(rect, 8 * rad, c.lighter_background);
        }
        p.text(
            r(16, y + 6, 26, 26),
            crate::fonts::icon("\u{f02e}", "B"),
            16,
            if on { c.accent } else { c.dark_foreground },
            true,
            false,
        );
        if m.rail {
            p.text(
                r(48, y, 108, 36),
                &label,
                12,
                c.dark_foreground,
                false,
                false,
            );
        }
        hit(m, rect, Action::Navigate(path));
        y += 40;
    }
    let devices = m.devices.list.clone();
    if !devices.is_empty() {
        p.rect(r(14, y + 6, rail - 28, 1), c.lighter_background);
        y += 18;
    }
    let busy = m.devices.busy_node().cloned();
    for d in devices {
        let on = d.mount.as_deref().is_some_and(|mp| m.path.starts_with(mp));
        let rect = r(8, y, rail - 16, 36);
        if on {
            p.rounded_rect(rect, 8 * rad, c.lighter_background);
        }
        let mounted = d.mount.is_some();
        let tint = if on {
            c.accent
        } else if mounted {
            c.foreground
        } else {
            c.dark_foreground
        };
        p.text(
            r(16, y + 6, 26, 26),
            crate::fonts::icon(if d.removable { "\u{f287}" } else { "\u{f0a0}" }, "D"),
            16,
            tint,
            true,
            false,
        );
        if m.rail {
            let detail = if busy.as_ref() == Some(&d.node) {
                "…".to_string()
            } else {
                match (d.mount.is_some(), d.free) {
                    (true, Some(free)) => format!("{} libres", files_core::format_size(free)),
                    (true, None) => "montado".into(),
                    (false, _) => "sin montar".into(),
                }
            };
            p.text(r(48, y, 82, 20), &d.title(), 12, tint, false, false);
            p.text(
                r(48, y + 18, 82, 16),
                &detail,
                9,
                c.dark_foreground,
                false,
                false,
            );
        }
        hit(m, rect, Action::Device(d.node.clone(), None));
        // Eject / unmount button for mounted devices (expanded sidebar).
        if mounted && m.rail {
            let eject = r(rail - 38, y + 6, 24, 24);
            p.text(
                eject,
                crate::fonts::icon("\u{f052}", "^"),
                13,
                c.dark_foreground,
                true,
                false,
            );
            hit(
                m,
                eject,
                Action::Device(
                    d.node.clone(),
                    Some(if d.removable {
                        files_core::devices::Op::Eject
                    } else {
                        files_core::devices::Op::Unmount
                    }),
                ),
            );
        }
        y += 40;
    }
    p.unclip();
    let rail_bottom = trash_y - 8;
    for target in &mut m.hits[rail_hits_start..] {
        let end = (target.rect.y + target.rect.h).min(rail_bottom);
        target.rect.y = target.rect.y.max(90);
        target.rect.h = (end - target.rect.y).max(0);
    }
    m.hits.retain(|hit| hit.rect.h > 0);
    p.rect(r(14, trash_y - 5, rail - 28, 1), c.lighter_background);
    p.text(
        r(18, trash_y, 30, 36),
        crate::fonts::icon("", "TR"),
        20,
        c.dark_foreground,
        true,
        false,
    );
    if m.rail {
        p.text(
            r(48, trash_y, 110, 36),
            "Papelera",
            12,
            c.dark_foreground,
            false,
            false,
        );
    }
    hit(m, r(8, trash_y, rail - 16, 36), Action::OpenTrash);
    let iw = if !m.dual.enabled && m.inspector && w >= 800 {
        240
    } else {
        0
    };
    let left = rail + 16;
    let right = w - iw - 16;
    let width = right - left;
    let top = 94;
    let bottom = crate::operation_bar::browser_height(m, h) - 40;
    let vh = bottom - top;
    m.listing_rect = r(left, top, width, vh);
    m.native_transfer
        .painted(m.pane_owner(), m.generation, r(left, top, width, vh));
    p.clip(r(left - 4, top, width + 8, vh));
    let mut y = top - m.scroll;
    if m.dual.enabled {
        m.dual_width = width;
        crate::dual_view::paint(p, m, r(left, top, width, vh));
        y = top - m.scroll + m.content_height;
    } else if m.view == View::Columns {
        crate::columns_view::paint(p, m, r(left, top, width, vh));
        y = top - m.scroll + m.content_height;
    } else if m.view == View::SpaceMap {
        crate::space_map_view::paint(p, m, r(left, top, width, vh));
        y = top - m.scroll + m.content_height;
    } else if m.view == View::Grid {
        crate::grid_view::paint(p, m, r(left, top, width, vh));
        y = top - m.scroll + m.content_height;
    } else if m.view == View::Spatial {
        crate::spatial_view::paint(p, m, r(left, top, width, vh));
        y = top - m.scroll + m.content_height;
    } else if m.view == View::List {
        crate::sort_controls::paint(
            p,
            r(left, y, width, 26),
            &c,
            &mut m.hits,
            crate::sort_controls::Header {
                sort: m.sort,
                descending: m.descending,
                generation: m.generation,
                pane: None,
                query: &m.query,
            },
        );
        y += 30;
        let first = (((top - y).max(0) / 38) as usize).min(m.order.len());
        let end = ((bottom - y).max(0) as usize)
            .div_ceil(38)
            .min(m.order.len());
        for pos in first..end {
            let i = m.order[pos];
            row(p, m, i, r(left, y + pos as i32 * 38, width, 38));
        }
        y += m.order.len() as i32 * 38;
    } else {
        let folders = m.group_ends[0];
        if folders > 0 {
            let header = format!("CARPETAS · {folders}");
            p.text(
                r(left, y, width, 24),
                &header,
                10,
                c.dark_foreground,
                false,
                false,
            );
            // Nautilus-like quick action next to the section title.
            let bx = left + p.text_width(&header, 10, false).ceil() as i32 + 14;
            // Nerd Font "folder plus" (nf-md-folder_plus), no text.
            let button = r(bx, y, 28, 24);
            if y + 24 > top && y < bottom && bx + 28 <= left + width {
                p.rounded_rect(button, 4 * m.radius, c.selection);
                p.text(
                    r(bx + 6, y, 22, 24),
                    "\u{f0257}",
                    14,
                    c.accent,
                    false,
                    false,
                );
                m.hits.push(Hit {
                    generation: m.generation,
                    rect: button,
                    action: Action::NewEntry(true),
                });
            }
            y += 28;
            let cols = (width / 240).max(1);
            let cw = (width - 8 * (cols - 1)) / cols;
            let first_row = ((top - y).max(0) / 82) as usize;
            let last_row = ((bottom - y).max(0) as usize).div_ceil(82);
            let start = first_row.saturating_mul(cols as usize).min(folders);
            let end = last_row.saturating_mul(cols as usize).min(folders);
            for pos in start..end {
                let i = m.order[pos];
                let tx = left + (pos as i32 % cols) * (cw + 8);
                let ty = y + (pos as i32 / cols) * 82;
                if ty + 74 > top && ty < bottom {
                    tile(p, m, i, r(tx, ty, cw, 74));
                }
            }
            y += ((folders as i32 + cols - 1) / cols) * 82 + 6;
        }
        for (g, label) in ["HOY", "ESTA SEMANA", "ANTES", "POR FECHAR"]
            .iter()
            .enumerate()
        {
            let start = m.group_ends[g];
            let end = m.group_ends[g + 1];
            let count = end - start;
            if count == 0 {
                continue;
            }
            p.text(
                r(left, y, width, 24),
                &format!("{label} · {count}"),
                10,
                c.dark_foreground,
                false,
                false,
            );
            y += 26;
            let first = (((top - y).max(0) / 38) as usize).min(count);
            let last = ((bottom - y).max(0) as usize).div_ceil(38).min(count);
            for rank in first..last {
                let i = m.order[start + rank];
                row(p, m, i, r(left, y + rank as i32 * 38, width, 38));
            }
            y += count as i32 * 38 + 10;
        }
    }
    if m.order.is_empty() && m.view != View::SpaceMap {
        p.text(
            r(left, top + 80, width, 30),
            if m.loaded {
                "No hay elementos"
            } else {
                "Leyendo carpeta…"
            },
            15,
            c.dark_foreground,
            false,
            false,
        );
    }
    m.content_height = (y + m.scroll - top).max(0);
    p.unclip();
    for target in &mut m.hits {
        if matches!(target.action, Action::Entry(_)) {
            let end = (target.rect.y + target.rect.h).min(bottom);
            target.rect.y = target.rect.y.max(top);
            target.rect.h = (end - target.rect.y).max(0);
        }
    }
    if m.content_height > vh {
        let sh = (vh * vh / m.content_height).max(24);
        let sy = top + (vh - sh) * m.scroll / (m.content_height - vh).max(1);
        p.rounded_rect(r(right + 7, sy, 3, sh), 1, c.muted);
    }
    if iw > 0 {
        inspector(p, m, w, crate::operation_bar::browser_height(m, h), iw);
    }
    crate::tabs_ui::paint(p, m, w);
    crate::operation_bar::paint(p, m, w, h);
    status(p, m, w, h);
    if m.settings && !m.toolkit_popup {
        let sx = (w - 330).max(0);
        let sy = 58;
        let compact = h < 500;
        let py = |normal: i32, small: i32| sy + if compact { small } else { normal };
        let height = if compact { 364 } else { 438 };
        hit(m, r(sx, sy, 300, height), Action::None);
        p.rounded_rect(r(sx, sy, 300, height), 10 * rad, c.muted);
        p.rounded_rect(
            r(sx + 1, sy + 1, 298, height - 2),
            10 * rad,
            c.darker_background,
        );
        p.text(
            r(sx + 16, sy + 6, 270, 26),
            "PREFERENCIAS · VENTANA",
            11,
            c.foreground,
            false,
            false,
        );
        for (j, mode) in [WindowMode::Normal, WindowMode::Fullscreen]
            .into_iter()
            .enumerate()
        {
            button(
                p,
                m,
                r(sx + 12, sy + 36 + j as i32 * 32, 276, 28),
                mode.label(),
                m.window_mode == mode,
                Action::WindowMode(mode),
            );
        }
        button(
            p,
            m,
            r(sx + 12, sy + 100, 276, 28),
            "  Mostrar ocultos · Ctrl+H",
            m.show_hidden,
            Action::ToggleHidden,
        );
        button(
            p,
            m,
            r(sx + 12, sy + 132, 134, 28),
            crate::fonts::icon(" Doble · F9", "Doble · F9"),
            m.dual.enabled,
            Action::DualPane,
        );
        button(
            p,
            m,
            r(sx + 150, sy + 132, 138, 28),
            " Barra MC",
            m.operation_bar,
            Action::ToggleOperationBar,
        );
        p.text(
            r(sx + 16, py(168, 162), 270, 18),
            "BORDES DE LA INTERFAZ",
            10,
            c.dark_foreground,
            false,
            false,
        );
        for (j, (label, corner)) in [
            ("Omarchy", Corners::Omarchy),
            ("Cuadrados", Corners::Square),
            ("Redondos", Corners::Rounded),
        ]
        .into_iter()
        .enumerate()
        {
            button(
                p,
                m,
                r(sx + 12 + j as i32 * 92, py(196, 182), 88, 30),
                label,
                m.corners == corner,
                Action::Corner(corner),
            );
        }
        p.text(
            r(sx + 16, py(238, 214), 270, 18),
            "TEMA",
            10,
            c.dark_foreground,
            false,
            false,
        );
        button(
            p,
            m,
            r(sx + 12, py(266, 234), 276, 30),
            "Sistema · en vivo",
            !m.alternate_theme,
            Action::Theme(false),
        );
        button(
            p,
            m,
            r(sx + 12, py(300, 266), 276, 30),
            "Tokyo Night · vista local",
            m.alternate_theme,
            Action::Theme(true),
        );
        button(
            p,
            m,
            r(sx + 12, py(344, 298), 276, 28),
            if m.dual.enabled {
                "  Guardar sólo este grupo"
            } else {
                "  Guardar sesión"
            },
            false,
            Action::SaveSession,
        );
        button(
            p,
            m,
            r(sx + 12, py(408, 330), 134, 28),
            " Guardar vista",
            false,
            Action::SaveFolderView,
        );
        button(
            p,
            m,
            r(sx + 150, py(408, 330), 138, 28),
            " Olvidar vista",
            false,
            Action::ForgetFolderView,
        );
        if !compact {
            button(
                p,
                m,
                r(sx + 12, sy + 376, 276, 28),
                "  Ayuda de atajos · F1",
                false,
                Action::Help,
            );
        }
    }
    if m.actions_menu && !m.toolkit_popup {
        let sy = 58;
        let items = [
            ("  Terminal aquí", Action::Terminal),
            ("  Comandos · Ctrl+Shift+P", Action::Commands),
            ("  Gestor de tamaños", Action::Sizes),
            ("  SFTP · montaje", Action::Sftp),
            ("󰇚  Extraer ZIP: revisar", Action::ExtractArchive),
            ("󰆧 Crear ZIP: revisar", Action::CreateArchive),
            (
                if m.sizes.paused() {
                    "  Reanudar tamaños"
                } else {
                    "  Pausar siguientes tamaños"
                },
                Action::PauseSizes(!m.sizes.paused()),
            ),
            ("  SHA-256 · comparar archivos", Action::Checksums),
            ("  Duplicados en esta carpeta", Action::Duplicates),
            ("  Reintentar tamaños fallidos", Action::RetrySizeErrors),
            ("󰄬  Selección…", Action::SelectionMenu(w, h)),
            ("  Vista previa · Space", Action::Preview),
            ("  Propiedades · Alt+Enter", Action::Properties),
            ("  Copiar · Ctrl+C (files)", Action::ClipboardCopy),
            ("  Movimientos interrumpidos", Action::RecoveryCatalog),
            ("  Cortar · Ctrl+X (files)", Action::ClipboardCut),
            ("  Renombrar · F2", Action::Rename),
            ("  Enviar a Papelera · Supr", Action::Trash),
            ("  Crear archivo · Ctrl+N", Action::NewEntry(false)),
            ("  Crear carpeta · F7", Action::NewEntry(true)),
            ("  Deshacer · Ctrl+Z", Action::Undo(false)),
            ("  Rehacer · Ctrl+Shift+Z", Action::Undo(true)),
            ("  Pegar en files · Ctrl+V", Action::ClipboardPaste),
        ];
        let available = (h - sy - 40).max(1);
        let columns = if (available - 46) / (items.len() as i32) < 22 && w >= 400 {
            2usize
        } else {
            1
        };
        let rows = items.len().div_ceil(columns);
        let step = ((available - 46) / rows as i32).clamp(18, 30);
        let menu_width = if columns == 2 { (w - 16).min(600) } else { 300 };
        let sx = if columns == 2 {
            (w - menu_width - 8).max(8)
        } else {
            (w - 330).max(0)
        };
        let column_width = menu_width / columns as i32;
        let height = 46 + rows as i32 * step;
        hit(m, r(sx, sy, menu_width, height), Action::None);
        p.rounded_rect(r(sx, sy, menu_width, height), 10 * rad, c.muted);
        p.rounded_rect(
            r(sx + 1, sy + 1, menu_width - 2, height - 2),
            10 * rad,
            c.darker_background,
        );
        p.text(
            r(sx + 16, sy + 6, menu_width - 32, 26),
            "ACCIONES · ARCHIVOS",
            11,
            c.foreground,
            false,
            false,
        );
        for (j, (label, action)) in items.into_iter().enumerate() {
            let label = if column_width < 260 {
                match &action {
                    Action::Commands => "  Comandos…",
                    Action::Duplicates => " Duplicados",
                    Action::Checksums => " SHA-256",
                    Action::PauseSizes(true) => "  Pausar tamaños",
                    Action::PauseSizes(false) => "  Reanudar tamaños",
                    Action::RetrySizeErrors => "  Reintentar fallidos",
                    Action::ExtractArchive => "󰇚  Extraer ZIP",
                    Action::CreateArchive => "󰆧  Crear ZIP",
                    Action::Properties => "  Propiedades",
                    Action::ClipboardCopy => "  Copiar (files)",
                    Action::ClipboardCut => "  Cortar (files)",
                    Action::ClipboardPaste => "  Pegar (files)",
                    Action::RecoveryCatalog => "  Recuperar mov.",
                    Action::Trash => "  Papelera · Supr",
                    Action::NewEntry(false) => "  Crear archivo",
                    Action::NewEntry(true) => "  Crear carpeta",
                    Action::Undo(true) => "  Rehacer",
                    _ => label,
                }
            } else {
                label
            };
            button(
                p,
                m,
                r(
                    sx + (j / rows) as i32 * column_width + 12,
                    sy + 36 + (j % rows) as i32 * step,
                    column_width - 24,
                    step - 2,
                ),
                label,
                false,
                action,
            );
        }
    }
    crate::native_drop::paint(p, m);
    crate::window_chrome::paint_grips(p, m, w, h);
    if (m.saves.last_error().is_some() || m.config_error.is_some() || m.session.error.is_some())
        && !m.closing
    {
        button(
            p,
            m,
            r(w - 180, h - 68, 164, 30),
            "Reintentar guardado",
            false,
            Action::RetrySaves,
        );
    }
    if !m.closing {
        crate::copy_ui::paint(p, m, w, h);
        crate::text_preview::paint(p, m, w, h);
        if let Some(band) = m.marquee.as_ref().filter(|b| b.moved) {
            // Rubber band: accent outline over a light tint.
            let b = band.rect();
            let c = m.palette.clone();
            p.clip(m.listing_rect);
            for edge in [
                r(b.x, b.y, b.w, 1),
                r(b.x, b.y + b.h - 1, b.w, 1),
                r(b.x, b.y, 1, b.h),
                r(b.x + b.w - 1, b.y, 1, b.h),
            ] {
                p.rect(edge, c.accent);
            }
            p.unclip();
        }
        crate::mutations::paint(p, m, w, h);
        crate::properties_ui::paint(p, m, w, h);
        if !m.toolkit_popup {
            crate::context_menu::paint(p, m, w, h);
        }
        crate::help_view::paint(p, m, w, h);
        crate::search_ui::paint(p, m, w, h);
        crate::command_palette::paint(p, m, w, h);
        crate::size_panel::paint(p, m, w, h);
        crate::checksum_ui::paint(p, m, w, h);
        crate::directory_compare_ui::paint(p, m, w, h);
        crate::bookmarks_ui::paint(p, m, w, h);
        crate::saved_search_ui::paint(p, m, w, h);
        crate::sftp_ui::paint(p, m, w, h);
        crate::trash_browser::paint(p, m, w, h);
        crate::delete_ui::paint(p, m, w, h);
        crate::open_with_ui::paint(p, m, w, h);
    }
    if m.closing {
        // Clear underlying targets: only explicit close/retry actions remain active.
        m.hits.clear();
        let cw = (w - 24).min(640);
        let compact = cw < 600;
        let ch = if compact { 330 } else { 240 };
        let x = (w - cw) / 2;
        let y = (h - ch) / 2;
        p.rounded_rect(r(x, y, cw, ch), 10 * rad, c.muted);
        p.rounded_rect(
            r(x + 1, y + 1, cw - 2, ch - 2),
            10 * rad,
            c.darker_background,
        );
        p.text(
            r(x + 20, y + 12, cw - 40, 40),
            if m.mutations.busy {
                "Cancelando tarea y esperando resultado…"
            } else {
                "Esperando guardados y tareas…"
            },
            15,
            c.bright_foreground,
            true,
            false,
        );
        let message = m
            .saves
            .last_error()
            .map(|e| format!("{}: {}", e.directory.display(), e.message))
            .unwrap_or_else(|| {
                "Los cambios permanecen en memoria hasta confirmar su escritura.".into()
            });
        let message = m
            .session
            .error
            .as_ref()
            .or(m.config_error.as_ref())
            .or_else(|| {
                if m.sftp_profiles.needs_resolution() {
                    Some(&m.sftp_profiles.error)
                } else if m.saved_searches.needs_resolution() {
                    Some(&m.saved_searches.error)
                } else if m.folder_views.needs_resolution() {
                    Some(&m.folder_views.error)
                } else if m.bookmarks.needs_resolution() {
                    Some(&m.bookmarks.error)
                } else {
                    None
                }
            })
            .unwrap_or(&message);
        p.text(
            r(x + 20, y + 58, cw - 40, if compact { 100 } else { 78 }),
            message,
            12,
            c.dark_foreground,
            true,
            false,
        );
        button(
            p,
            m,
            r(
                x + 20,
                y + if compact { 170 } else { 156 },
                if compact { cw - 40 } else { 170 },
                36,
            ),
            "Seguir trabajando",
            false,
            Action::CancelClose,
        );
        button(
            p,
            m,
            r(
                x + if compact { 20 } else { 206 },
                y + if compact { 218 } else { 156 },
                if compact { cw - 40 } else { 130 },
                36,
            ),
            "Reintentar",
            false,
            Action::RetrySaves,
        );
        button(
            p,
            m,
            r(
                x + if compact { 20 } else { 350 },
                y + if compact { 266 } else { 156 },
                if compact { cw - 40 } else { 270 },
                36,
            ),
            "Cerrar sin guardar pendientes",
            false,
            Action::DiscardClose,
        );
    }
}
fn tile(p: &mut dyn Painter, m: &mut Model, i: usize, rect: Rect) {
    let c = m.palette.clone();
    let on = m.selection.contains(&i) || m.selected == Some(i);
    p.rounded_rect(
        rect,
        11 * m.radius,
        if on { c.accent } else { c.lighter_background },
    );
    let inner = r(rect.x + 1, rect.y + 1, rect.w - 2, rect.h - 2);
    p.rounded_rect(
        inner,
        10 * m.radius,
        if on {
            c.selection
        } else {
            c.lighter_background
        },
    );
    if m.selected == Some(i) {
        p.rect(r(rect.x + 4, rect.y + 8, 2, (rect.h - 16).max(0)), c.accent);
    }
    folder(p, rect.x + 12, rect.y + 22, c.blue);
    let e = &m.entries[i];
    p.text(
        r(rect.x + 54, rect.y + 10, rect.w - 64, 24),
        &files_core::name_label::label(&e.name),
        13,
        c.bright_foreground,
        false,
        false,
    );
    let summary = m.sizes.detail(&m.path, &e.name).unwrap_or_else(|| {
        m.summaries
            .get(&i)
            .map(|summary| summary.label())
            .unwrap_or_else(|| "contando…".into())
    });
    p.text(
        r(rect.x + 54, rect.y + 34, rect.w - 64, 18),
        &summary,
        11,
        c.dark_foreground,
        false,
        false,
    );
    let age = e
        .metadata
        .as_ref()
        .map(|m| m.modified)
        .map(|t| relative_date(t, SystemTime::now()))
        .unwrap_or_else(|| "—".into());
    p.text(
        r(rect.x + 54, rect.y + 51, rect.w - 64, 16),
        &age,
        10,
        c.dark_foreground,
        false,
        false,
    );
    hit(m, rect, Action::Entry(i));
    m.visible.push(i);
}
fn row(p: &mut dyn Painter, m: &mut Model, i: usize, rect: Rect) {
    let c = m.palette.clone();
    let columns = crate::list_columns::Layout::new(rect.w, crate::list_columns::Profile::Single);
    let e = &m.entries[i];
    if m.selection.contains(&i) || m.selected == Some(i) || m.hovered == Some(i) {
        p.rounded_rect(
            rect,
            9 * m.radius,
            if m.selected == Some(i) {
                c.selection
            } else {
                c.lighter_background
            },
        );
    }
    if m.selected == Some(i) {
        p.rect(r(rect.x + 1, rect.y + 4, 2, (rect.h - 8).max(0)), c.accent);
    }
    let x = rect.x;
    let y = rect.y;
    let path = m.path.join(&e.name);
    if e.is_dir() {
        folder(p, x + 8, y + 4, c.blue);
    } else {
        let bc = badge_color(m, &path);
        p.rounded_rect(r(x + 8, y + 5, 28, 28), m.badge_radius, bc);
        let ext = path
            .extension()
            .map(|s| match files_core::name_label::label(s) {
                std::borrow::Cow::Borrowed(text) => text.to_ascii_uppercase(),
                std::borrow::Cow::Owned(text) => text,
            })
            .unwrap_or_else(|| "FILE".into());
        p.text(
            r(x + 10, y + 5, 25, 28),
            &ext,
            9,
            c.darker_background,
            true,
            false,
        );
    }
    p.text(
        r(x + columns.name.x, y, columns.name.w, 38),
        &files_core::name_label::label(&e.name),
        13,
        c.bright_foreground,
        false,
        false,
    );
    if e.is_dir()
        && let Some(cell) = columns.size
        && let Some(size) = m.sizes.label(&m.path, &e.name)
    {
        p.text(
            r(x + cell.x, y, cell.w, 38),
            &size,
            11,
            if matches!(
                m.sizes.status(&m.path, &e.name),
                Some(crate::size_queue::Status::Failed(_))
            ) {
                c.red
            } else {
                c.accent
            },
            true,
            true,
        );
    }
    if let Some(meta) = &e.metadata {
        if !e.is_dir()
            && let Some(cell) = columns.size
        {
            let (n, u) = size_parts(meta.size);
            p.text(
                r(x + cell.x, y, cell.w - 32, 38),
                &n,
                12,
                c.dark_foreground,
                true,
                true,
            );
            p.text(
                r(x + cell.x + cell.w - 28, y, 28, 38),
                u,
                11,
                c.dark_foreground,
                true,
                false,
            );
        }
        if let Some(cell) = columns.date {
            p.text(
                r(x + cell.x, y, cell.w, 38),
                &relative_date(meta.modified, SystemTime::now()),
                11,
                c.dark_foreground,
                false,
                true,
            );
        }
    }
    hit(m, rect, Action::Entry(i));
    m.visible.push(i);
}
fn inspector(p: &mut dyn Painter, m: &mut Model, w: i32, h: i32, iw: i32) {
    let c = m.palette.clone();
    let x = w - iw;
    p.rect(r(x, 90, iw, h - 122), c.darker_background);
    p.rect(r(x, 90, 1, h - 122), c.lighter_background);
    p.text(
        r(x + 16, 102, iw - 32, 22),
        "INSPECTOR",
        10,
        c.dark_foreground,
        false,
        false,
    );
    let Some(i) = m.selected else {
        p.text(
            r(x + 16, 142, iw - 32, 28),
            "Selecciona un elemento",
            12,
            c.dark_foreground,
            false,
            false,
        );
        return;
    };
    let Some(e) = m.entries.get(i) else {
        return;
    };
    let path = m.path.join(&e.name);
    let color = if e.is_dir() {
        c.blue
    } else {
        badge_color(m, &path)
    };
    p.clip(r(x + 8, 98, iw - 16, (h - 174 - 98).max(0)));
    let preview_h = ((h - 424) / 2).clamp(38, 98);
    let title_y = 152 + preview_h;
    let icon_y = 140 + (preview_h - 48) / 2;
    p.rounded_rect(
        r(x + 16, 140, iw - 32, preview_h),
        m.badge_radius,
        c.lighter_background,
    );
    let mut preview_palette = c.clone();
    preview_palette.background = c.lighter_background;
    if m.thumbnails.paint(
        p,
        &path,
        r(x + 16, 140, iw - 32, preview_h),
        &preview_palette,
    ) {
        // Shared borrowed bitmap; no second cache allocation for inspector.
    } else if e.is_dir() {
        folder(p, x + 100, icon_y + 10, color);
    } else {
        p.rounded_rect(r(x + 96, icon_y, 48, 48), 10, color);
        let ext = path
            .extension()
            .map(|s| match files_core::name_label::label(s) {
                std::borrow::Cow::Borrowed(text) => text.to_ascii_uppercase(),
                std::borrow::Cow::Owned(text) => text,
            })
            .unwrap_or_else(|| "FILE".into());
        p.text(
            r(x + 100, icon_y, 40, 48),
            &ext,
            14,
            c.darker_background,
            true,
            false,
        );
    }
    if m.thumbnails.error(&path).is_some() {
        p.text(
            r(x + 20, 140 + preview_h - 20, iw - 40, 18),
            "Miniatura no disponible",
            10,
            c.red,
            true,
            false,
        );
    }
    p.text(
        r(x + 16, title_y, iw - 32, 36),
        &files_core::name_label::label(&e.name),
        13,
        c.bright_foreground,
        false,
        false,
    );
    let compact_directory_detail = (e.is_dir() && title_y + 94 > h - 174)
        .then(|| {
            let label = m.sizes.label(&m.path, &e.name)?;
            Some(match m.sizes.status(&m.path, &e.name)? {
                crate::size_queue::Status::Measured { entries, .. } => {
                    format!("{label} · {entries} entradas")
                }
                _ => label,
            })
        })
        .flatten();
    p.text(
        r(x + 16, title_y + 44, iw - 32, 22),
        if let Some(detail) = compact_directory_detail.as_deref() {
            detail
        } else if e.is_dir() {
            "Carpeta"
        } else {
            mime::mime(&path)
        },
        11,
        c.dark_foreground,
        false,
        false,
    );
    if let Some(details) =
        crate::space_map_view::inspector_details(m, i).filter(|_| m.view == View::SpaceMap)
    {
        p.text(
            r(x + 16, title_y + 72, iw - 32, 78),
            &details,
            11,
            c.foreground,
            false,
            false,
        );
    } else {
        if let Some(meta) = &e.metadata {
            if !e.is_dir() && title_y + 94 <= h - 174 {
                p.text(
                    r(x + 16, title_y + 72, iw - 32, 22),
                    &format!("Tamaño    {}", format_size(meta.size)),
                    12,
                    c.foreground,
                    false,
                    false,
                );
            }
            if !e.is_dir() && title_y + 120 <= h - 174 {
                p.text(
                    r(x + 16, title_y + 98, iw - 32, 22),
                    &relative_date(meta.modified, SystemTime::now()),
                    12,
                    c.foreground,
                    false,
                    false,
                );
            }
        }
        if e.is_dir() && title_y + 94 <= h - 174 {
            let detail = m.sizes.detail(&m.path, &e.name);
            let text = detail.as_deref().unwrap_or("Tamaño sin calcular");
            let (size, count) = text.split_once(" · ").unwrap_or((text, "No sigue enlaces"));
            p.text(
                r(x + 16, title_y + 72, iw - 32, 22),
                size,
                11,
                c.foreground,
                false,
                false,
            );
            p.text(
                r(x + 16, title_y + 98, iw - 32, 22),
                count,
                11,
                c.dark_foreground,
                false,
                false,
            );
        }
    }
    let is_directory = e.is_dir();
    if crate::thumbnail_job::supported(&path) && !is_directory {
        m.hits.push(Hit {
            rect: r(x + 16, 140, iw - 32, preview_h),
            action: Action::Preview,
            generation: m.generation,
        });
    }
    if is_directory && m.view != View::SpaceMap && title_y + 164 <= h - 174 {
        let name = m
            .selected
            .and_then(|i| m.entries.get(i))
            .map(|e| e.name.as_os_str());
        let (label, action) = match name.and_then(|n| m.sizes.status(&m.path, n)) {
            Some(crate::size_queue::Status::Queued | crate::size_queue::Status::Running { .. }) => {
                (
                    " Cancelar cálculo",
                    Action::CancelSize(name.expect("directory status has a name").to_os_string()),
                )
            }
            Some(crate::size_queue::Status::Measured { .. }) => {
                (" Recalcular · Ctrl+Espacio", Action::CalculateSize)
            }
            _ => (" Calcular · Ctrl+Espacio", Action::CalculateSize),
        };
        button(
            p,
            m,
            r(x + 16, title_y + 130, iw - 32, 30),
            label,
            false,
            action,
        );
    } else if m.view != View::SpaceMap && title_y + 164 <= h - 174 {
        p.text(
            r(x + 16, title_y + 130, iw - 32, 28),
            if m.filter_edit.active {
                "Enter vuelve a la lista"
            } else {
                "Enter abre · Backspace sube"
            },
            11,
            c.dark_foreground,
            false,
            false,
        );
    }
    p.unclip();
    button(
        p,
        m,
        r(x + 16, h - 168, iw - 32, 30),
        "󰑕  Renombrar · F2",
        false,
        Action::Rename,
    );
    button(
        p,
        m,
        r(x + 16, h - 126, (iw - 40) / 2, 34),
        "Abrir",
        true,
        Action::Open,
    );
    button(
        p,
        m,
        r(x + 24 + (iw - 40) / 2, h - 126, (iw - 40) / 2, 34),
        " Previa",
        false,
        Action::Preview,
    );
    button(
        p,
        m,
        r(x + 16, h - 84, 60, 34),
        crate::fonts::icon("", "Ruta"),
        false,
        Action::CopyPath,
    );
    button(
        p,
        m,
        r(x + 80, h - 84, 68, 34),
        if crate::selection_tools::is_map(m) {
            crate::fonts::icon("", "Copiar")
        } else {
            crate::fonts::icon(" F5", "Copiar")
        },
        false,
        Action::CopyFiles,
    );
    button(
        p,
        m,
        r(x + 152, h - 84, 72, 34),
        crate::fonts::icon(" F6", "Mover"),
        false,
        Action::MoveFiles,
    );
}
pub(crate) fn entry_top(m: &Model, i: usize, w: i32) -> Option<i32> {
    let rank = m.order.iter().position(|&j| j == i)?;
    if m.dual.enabled {
        return Some(rank as i32 * 36);
    }
    if m.view == View::Grid {
        return Some(
            crate::grid_view::Metrics::new(
                crate::grid_view::width(m, w),
                m.grid_zoom,
                m.order.len(),
            )
            .row_top(rank),
        );
    }
    if m.view == View::Spatial {
        return Some((m.spatial_position(i).y * m.layout.zoom) as i32);
    }
    if m.view == View::Columns {
        return Some(rank as i32 * 36);
    }
    if m.view == View::SpaceMap {
        return None;
    }
    let left = if m.rail { 186 } else { 68 };
    let iw = if !m.dual.enabled && m.inspector && w >= 800 {
        240
    } else {
        0
    };
    let width = w - iw - 16 - left;
    let mut y = 0;
    if m.view == View::List {
        y = 30 + 38 * m.order.iter().position(|&j| j == i).unwrap_or(0) as i32;
    } else {
        let folders = m.group_ends[0];
        let cols = (width / 240).max(1);
        if let Some(rank) = m.order.iter().position(|&j| j == i) {
            if rank < folders {
                y = 28 + (rank as i32 / cols) * 82;
            } else {
                if folders > 0 {
                    y = 28 + ((folders as i32 + cols - 1) / cols) * 82 + 6;
                }
                for g in 0..4 {
                    let start = m.group_ends[g];
                    let end = m.group_ends[g + 1];
                    if start == end {
                        continue;
                    }
                    y += 26;
                    if rank < end {
                        y += (rank - start) as i32 * 38;
                        break;
                    }
                    y += (end - start) as i32 * 38 + 10;
                }
            }
        }
    }
    Some(y)
}
/// Desplaza sólo lo necesario para mantener la selección dentro del área visible.
pub fn reveal_selection(m: &mut Model, w: i32, h: i32) {
    let Some(i) = m.selected else {
        return;
    };
    if m.dual.enabled {
        crate::dual_view::reveal(m, h);
        return;
    }
    if m.view == View::Columns {
        crate::columns_view::reveal(m, h);
        return;
    }
    if m.view == View::SpaceMap {
        crate::space_map_view::reveal(m);
        return;
    }
    if m.view == View::Grid {
        crate::grid_view::reveal(m, w, h);
        return;
    }
    let h = crate::operation_bar::browser_height(m, h);
    if m.view == View::Spatial {
        let position = m.spatial_position(i);
        let y = (position.y * m.layout.zoom) as i32;
        if y < m.scroll {
            m.scroll = y;
        } else if y + 120 > m.scroll + h - 170 {
            m.scroll = (y + 120 - (h - 170)).max(0);
        }
        let x = position.x * m.layout.zoom;
        let available = (w
            - if m.inspector && w >= 800 { 240 } else { 0 }
            - if m.rail { 170 } else { 52 }
            - 32) as f32;
        if x < m.pan_x {
            m.pan_x = x;
        } else if x + 128.0 * m.layout.zoom > m.pan_x + available {
            m.pan_x = (x + 128.0 * m.layout.zoom - available).max(0.0);
        }
        return;
    }
    let Some(y) = entry_top(m, i, w) else {
        return;
    };
    let vh = h - 134;
    if y < m.scroll {
        m.scroll = y;
    } else {
        let item_height = if m.view == View::Groups && m.entries.get(i).is_some_and(|e| e.is_dir())
        {
            74
        } else {
            38
        };
        if y + item_height > m.scroll + vh {
            m.scroll = (y + item_height - vh).max(0);
        }
    }
}

// Glifos propios, sin cargar fuentes de iconos ni un atlas residente.
fn rail_icon(p: &mut dyn Painter, x: i32, y: i32, kind: usize, c: Color) {
    let glyph = match kind {
        0 => crate::fonts::icon("", "H"),
        1 => crate::fonts::icon("", "D"),
        2 => crate::fonts::icon("", "T"),
        3 => crate::fonts::icon("", "I"),
        4 => crate::fonts::icon("", "M"),
        5 => crate::fonts::icon("", "V"),
        _ => crate::fonts::icon("", "FS"),
    };
    p.text(r(x - 2, y - 4, 26, 28), glyph, 18, c, true, false);
}

#[cfg(test)]
mod inspector_tabs_tests {
    use super::*;
    #[derive(Default)]
    struct Recorder {
        text: Vec<(Rect, String)>,
    }
    impl Painter for Recorder {
        fn rect(&mut self, _: Rect, _: Color) {}
        fn rounded_rect(&mut self, _: Rect, _: i32, _: Color) {}
        fn text(&mut self, r: Rect, t: &str, _: i32, _: Color, _: bool, _: bool) {
            self.text.push((r, t.into()));
        }
        fn image(&mut self, _: Rect, _: &crate::painter::Image<'_>) {}
        fn clip(&mut self, _: Rect) {}
        fn unclip(&mut self) {}
    }
    #[test]
    fn trash_sidebar_is_pinned_and_catalog_targets_are_bounded_at_minimum_size() {
        for operation_bar in [false, true] {
            let mut m = Model::new("/fixture-trash-layout".into());
            m.loaded = true;
            m.operation_bar = operation_bar;
            for rail in [false, true] {
                m.rail = rail;
                paint(&mut Recorder::default(), &mut m, 440, 440);
                let trash = m
                    .hits
                    .iter()
                    .find(|h| matches!(h.action, Action::OpenTrash))
                    .unwrap();
                assert!(
                    trash.rect.y >= 90
                        && trash.rect.y + trash.rect.h
                            <= crate::operation_bar::browser_height(&m, 440) - 32
                );
                for hit in &m.hits {
                    if hit.rect.x < 52
                        && matches!(hit.action, Action::Navigate(_) | Action::Bookmarks)
                    {
                        assert!(hit.rect.y + hit.rect.h <= trash.rect.y);
                    }
                }
            }
            m.action(Action::OpenTrash);
            let id = m.trash_browser.as_ref().unwrap().id;
            assert!(
                m.receive_trash_catalog(
                    id,
                    Ok(files_core::trash::Catalog {
                        items: (0..1000)
                            .map(|i| files_core::trash::CatalogItem {
                                name: format!("file{i}.txt").into(),
                                original: Some(format!("/original/file{i}.txt").into()),
                                deletion_date: None,
                                issue: None,
                            })
                            .collect(),
                        limited: false,
                    })
                )
            );
            for (w, h) in [(440, 440), (700, 440), (1000, 650)] {
                paint(&mut Recorder::default(), &mut m, w, h);
                assert!(m.hits.len() > 2 && m.hits.len() < 20);
                assert!(
                    m.hits
                        .iter()
                        .all(|h| matches!(h.action, Action::TrashBrowser(..)))
                );
                assert!(m.hits.iter().all(|hit| hit.rect.x >= 12
                    && hit.rect.y >= 20
                    && hit.rect.x + hit.rect.w <= w - 12
                    && hit.rect.y + hit.rect.h <= h - 20));
            }
        }
    }
    #[test]
    fn narrow_list_rows_keep_names_before_optional_size_and_date() {
        let mut m = Model::new("/fixture-narrow-list".into());
        m.loaded = true;
        m.view = View::List;
        #[cfg(unix)]
        let name = {
            use std::os::unix::ffi::OsStringExt;
            std::ffi::OsString::from_vec(b"a\xff\n".to_vec())
        };
        #[cfg(not(unix))]
        let name = std::ffi::OsString::from("one.txt");
        m.entries = vec![files_core::listing::Entry {
            name: name.clone(),
            kind: files_core::listing::Kind::File,
            metadata: Some(files_core::listing::Metadata {
                size: 7,
                modified: std::time::UNIX_EPOCH,
            }),
            stat_attempted: true,
        }];
        m.selected = Some(0);
        m.selection.insert(0);
        for width in [230, 348, 676] {
            m.hits.clear();
            m.visible.clear();
            let mut p = Recorder::default();
            row(&mut p, &mut m, 0, Rect::new(10, 20, width, 38));
            let label = files_core::name_label::label(&name);
            let name_rect = p.text.iter().find(|(_, t)| t == label.as_ref()).unwrap().0;
            assert!(name_rect.w >= 160 && name_rect.x + name_rect.w <= 10 + width);
            assert!(p.text_width(&label, 13, false) <= name_rect.w as f32);
            assert_eq!(p.text.iter().any(|(_, t)| t == "7"), width >= 348);
            for (r, t) in &p.text {
                if t != "FILE" && t != label.as_ref() && r.x >= name_rect.x {
                    assert!(r.x >= name_rect.x + name_rect.w + 8 && r.x + r.w <= 10 + width);
                }
            }
            assert_eq!(m.visible, vec![0]);
            assert!(m.hits.iter().any(|h| matches!(h.action, Action::Entry(0))));
            assert_eq!(m.entries[0].name, name);
        }
    }
    #[test]
    #[cfg(unix)]
    fn file_rows_escape_raw_controls_without_changing_operation_names() {
        use std::os::unix::ffi::OsStringExt;
        let original = std::ffi::OsString::from_vec(b"raw\xff\n.txt".to_vec());
        let mut m = Model::new("/fixture-name-labels".into());
        m.loaded = true;
        m.entries = vec![files_core::listing::Entry {
            name: original.clone(),
            kind: files_core::listing::Kind::File,
            metadata: None,
            stat_attempted: true,
        }];
        for view in [View::Groups, View::List, View::Grid] {
            m.view = view;
            m.rebuild();
            let mut p = Recorder::default();
            paint(&mut p, &mut m, 1000, 650);
            assert!(
                p.text.iter().any(|(_, text)| text == "raw\\xFF\\n.txt"),
                "view must paint the escaped label"
            );
            assert_eq!(m.entries[0].name, original);
            assert!(m.hits.iter().any(|h| matches!(h.action, Action::Entry(0))));
        }
    }
    #[test]
    #[cfg(unix)]
    fn escaped_breadcrumb_and_tab_keep_exact_navigation_destinations() {
        use std::os::unix::ffi::OsStringExt;
        let path = PathBuf::from(std::ffi::OsString::from_vec(
            b"/fixture-header/raw\xff\n".to_vec(),
        ));
        let mut m = Model::new(path.clone());
        m.loaded = true;
        let mut p = Recorder::default();
        paint(&mut p, &mut m, 1000, 650);
        assert!(p.text.iter().filter(|(_, s)| s == "raw\\xFF\\n").count() >= 2);
        assert!(
            m.hits
                .iter()
                .any(|h| matches!(&h.action,Action::Navigate(target) if *target==path))
        );
        assert_eq!(m.path, path);
        assert_eq!(m.tabs.rows[0].path, path);
    }
    #[test]
    fn pending_save_close_controls_fit_minimum_window() {
        let mut m = Model::new("/fixture-close-layout".into());
        m.closing = true;
        m.config_error = Some("No se pudo guardar; reintenta o cierra sin pendientes".into());
        for (w, h) in [(440, 440), (800, 440), (1000, 650)] {
            paint(&mut Recorder::default(), &mut m, w, h);
            let controls: Vec<_> = m
                .hits
                .iter()
                .filter(|h| {
                    matches!(
                        h.action,
                        Action::CancelClose | Action::RetrySaves | Action::DiscardClose
                    )
                })
                .collect();
            assert_eq!(controls.len(), 3);
            for hit in controls {
                assert!(
                    hit.rect.x >= 0
                        && hit.rect.y >= 0
                        && hit.rect.w > 0
                        && hit.rect.h > 0
                        && hit.rect.x + hit.rect.w <= w
                        && hit.rect.y + hit.rect.h <= h
                );
            }
        }
    }
    #[test]
    fn groups_and_list_paint_only_viewport_hits_for_100k_without_index_vectors() {
        let mut m = Model::new("/fixture-only-groups-100k".into());
        m.loaded = true;
        m.entries = (0..100_000)
            .map(|i| files_core::listing::Entry {
                name: format!("item-{i:06}").into(),
                kind: if i < 50_000 {
                    files_core::listing::Kind::Directory
                } else {
                    files_core::listing::Kind::File
                },
                metadata: None,
                stat_attempted: false,
            })
            .collect();
        m.rebuild();
        assert_eq!(m.group_ends, [50_000, 50_000, 50_000, 50_000, 100_000]);
        for (view, w, h) in [
            (View::Groups, 440, 440),
            (View::Groups, 1000, 650),
            (View::List, 440, 440),
            (View::List, 1000, 650),
        ] {
            m.view = view;
            m.rebuild();
            for end in [false, true] {
                m.scroll = if end {
                    (m.content_height - (h - 134)).max(0)
                } else {
                    0
                };
                let mut p = Recorder::default();
                paint(&mut p, &mut m, w, h);
                assert!(!m.visible.is_empty());
                assert!(m.visible.len() < 80, "100k must not create a hit per entry");
                assert!(
                    p.text.len() < 800,
                    "painting must visit only visible rows/tiles"
                );
            }
        }
    }
    #[test]
    fn groups_navigation_can_paint_empty_before_any_new_worker_batch() {
        let mut m = Model::new("/fixture-only-group-old".into());
        m.loaded = true;
        m.entries = vec![files_core::listing::Entry {
            name: "old-folder".into(),
            kind: files_core::listing::Kind::Directory,
            metadata: None,
            stat_attempted: false,
        }];
        m.rebuild();
        assert_eq!(m.group_ends, [1, 1, 1, 1, 1]);
        m.load_location("/fixture-only-group-new".into());
        let mut p = Recorder::default();
        paint(&mut p, &mut m, 800, 440);
        assert!(m.order.is_empty() && m.visible.is_empty());
        assert_eq!(m.group_ends, [0; 5]);
        assert!(p.text.iter().any(|(_, text)| text == "Leyendo carpeta…"));
    }
    #[test]
    fn search_calendar_presets_fit_minimum_and_hide_result_activation() {
        let mut m = Model::new("/fixture-calendar".into());
        m.loaded = true;
        m.search
            .open("/fixture-calendar".into(), "txt".into(), false);
        m.search.toggle_calendar();
        for (w, h) in [(440, 440), (800, 500), (1000, 650)] {
            let mut p = Recorder::default();
            paint(&mut p, &mut m, w, h);
            assert_eq!(
                m.hits
                    .iter()
                    .filter(|hit| matches!(hit.action, Action::SearchDate(_)))
                    .count(),
                6
            );
            let choices: Vec<_> = m
                .hits
                .iter()
                .filter(|hit| matches!(hit.action, Action::SearchDate(_)))
                .collect();
            for (i, a) in choices.iter().enumerate() {
                assert!(a.rect.y + a.rect.h <= h - 144);
                for b in &choices[i + 1..] {
                    assert!(
                        a.rect.x + a.rect.w <= b.rect.x
                            || b.rect.x + b.rect.w <= a.rect.x
                            || a.rect.y + a.rect.h <= b.rect.y
                            || b.rect.y + b.rect.h <= a.rect.y
                    );
                }
            }
            assert!(!m.hits.iter().any(|hit| matches!(
                hit.action,
                Action::SearchSelect(..)
                    | Action::SearchToggle(..)
                    | Action::SearchOpen(_)
                    | Action::SearchMarkAll(_)
                    | Action::SearchTransfer(_)
            )));
            assert!(m.hits.iter().all(|hit| hit.rect.x >= 12
                && hit.rect.y >= 66
                && hit.rect.x + hit.rect.w <= w - 12
                && hit.rect.y + hit.rect.h <= h - 40));
        }
    }
    #[test]
    fn paged_content_controls_fit_and_keep_scope_counters_in_the_compact_footer() {
        let mut m = Model::new("/fixture-paged-content".into());
        m.loaded = true;
        m.search.open(m.path.clone(), "ext:txt".into(), false);
        m.search.page_mode = true;
        let page = files_core::search_page::Page {
            version: 1,
            id: 1,
            sequence: 0,
            found_total: 1,
            more: true,
            report: files_core::search::Report {
                hits: vec![files_core::search::Hit {
                    relative: "file.txt".into(),
                    kind: files_core::listing::Kind::File,
                    content: Some(files_core::search_content::Match {
                        line: 2,
                        byte: 7,
                        regex_end: None,
                    }),
                    snippet: Some(files_core::search_snippet::Snippet {
                        text: "needle text".into(),
                        start: 0,
                        end: 6,
                        leading: false,
                        trailing: false,
                        match_clipped: false,
                    }),
                }],
                visited: 1,
                content: Some(files_core::search_content::Stats {
                    files: 1,
                    bytes: 19,
                    ..Default::default()
                }),
                reason: Some("page".into()),
                ..Default::default()
            },
        };
        m.search
            .pages
            .admit(page, &mut m.search.report, Default::default())
            .unwrap();
        for (w, h) in [(440, 440), (800, 500), (1000, 650)] {
            let mut p = Recorder::default();
            paint(&mut p, &mut m, w, h);
            assert!(p.text.iter().any(|(_, s)| s.contains("L0 B0 Δ0 E0 M0")));
            assert!(
                p.text
                    .iter()
                    .any(|(_, s)| s.contains("Página") || s.contains("Marcar página"))
            );
            assert_eq!(
                m.hits
                    .iter()
                    .filter(|hit| matches!(hit.action, Action::SearchPageStep(1, _)))
                    .count(),
                1
            );
            assert!(
                !m.hits
                    .iter()
                    .any(|hit| matches!(hit.action, Action::SearchPageStep(-1, _)))
            );
            assert!(m.hits.iter().all(|hit| hit.rect.x >= 12
                && hit.rect.y >= 66
                && hit.rect.x + hit.rect.w <= w - 12
                && hit.rect.y + hit.rect.h <= h - 40));
            assert!(
                m.hits
                    .iter()
                    .any(|hit| matches!(hit.action, Action::SearchSelect(..)) && hit.rect.h == 64)
            );
        }
    }
    #[test]
    fn copy_policy_options_fit_minimum_and_return_to_review_without_enqueue() {
        let mut m = Model::new("/fixture-copy-policy".into());
        m.loaded = true;
        m.entries = vec![files_core::listing::Entry {
            name: "file".into(),
            kind: files_core::listing::Kind::File,
            metadata: None,
            stat_attempted: false,
        }];
        m.rebuild();
        m.select_entry(0, false, false);
        m.open_copy();
        m.action(Action::CopyOptions);
        for (w, h) in [(440, 440), (800, 440), (1000, 650)] {
            let mut p = Recorder::default();
            paint(&mut p, &mut m, w, h);
            assert_eq!(m.hits.len(), 3);
            assert!(m.hits.iter().all(|hit| hit.rect.x >= 20
                && hit.rect.y >= 20
                && hit.rect.x + hit.rect.w <= w - 20
                && hit.rect.y + hit.rect.h <= h - 20));
            assert!(m.hits.iter().all(|hit| matches!(
                hit.action,
                Action::CopyOptions | Action::CopyVisiblePublication | Action::CopySymlinkTimes
            )));
            assert!(p.text.iter().any(|(_, s)| s.contains("automáticamente")));
            for (rect, text) in &p.text {
                if text.contains("automáticamente") || text.starts_with("Copia visible:") {
                    assert!(
                        text.lines()
                            .all(|line| line.chars().count() as f32 * 11.0 * 0.6 <= rect.w as f32),
                        "policy warning line exceeds minimum viewport"
                    );
                }
            }
        }
        m.action(Action::CopyOptions);
        assert!(!m.copy_busy());
    }
    #[test]
    fn sftp_transport_controls_fit_minimum_without_claiming_connected() {
        let mut m = Model::new("/fixture-sftp".into());
        m.loaded = true;
        m.action(Action::Sftp);
        for (w, h) in [(440, 440), (800, 440), (1000, 650)] {
            let mut p = Recorder::default();
            paint(&mut p, &mut m, w, h);
            assert_eq!(m.hits.len(), 14);
            assert!(
                m.hits
                    .iter()
                    .all(|h| matches!(h.action, Action::SftpCommand(..)))
            );
            assert!(m.hits.iter().all(|hit| hit.rect.x >= 12
                && hit.rect.x + hit.rect.w <= w - 12
                && hit.rect.y >= 66
                && hit.rect.y + hit.rect.h <= h - 40));
            assert!(!p.text.iter().any(|(_, s)| s.contains("Conectado")));
        }
    }
    #[test]
    fn sftp_report_controls_and_wrapped_lines_fit_supported_sizes() {
        let mut m = Model::new("/fixture-report".into());
        m.loaded = true;
        m.action(Action::Sftp);
        m.sftp.transport.status.diagnostic = "long ñ ".repeat(500);
        let stamp = m.sftp.form.as_ref().unwrap().stamp;
        m.sftp_command(stamp, crate::sftp_ui::Command::Report);
        for (w, h) in [(440, 440), (1000, 650), (800, 440)] {
            let mut p = Recorder::default();
            paint(&mut p, &mut m, w, h);
            assert!(m.hits.len() >= 5 && m.hits.len() <= 7);
            assert!(m.hits.iter().all(|hit| hit.rect.x >= 12
                && hit.rect.x + hit.rect.w <= w - 12
                && hit.rect.y >= 66
                && hit.rect.y + hit.rect.h <= h - 40));
            let r = m.sftp.form.as_ref().unwrap().report.as_ref().unwrap();
            assert!(r.total() > r.visible_rows);
            assert!(r.ranges_for_paint().count() <= r.visible_rows);
        }
    }
    #[test]
    fn sftp_profiles_scroll_to_selected_and_fit_minimum_with_all_controls() {
        let mut m = Model::new("/fixture-sftp-profiles".into());
        m.loaded = true;
        m.action(Action::Sftp);
        m.sftp_profile_command(crate::sftp_ui::Command::Profiles);
        m.sftp_profiles.document = std::sync::Arc::new(files_core::sftp_profiles::Document {
            next: 33,
            rows: (1..=32)
                .map(|id| files_core::sftp_profiles::Entry {
                    id,
                    label: format!("Server{id}"),
                    criteria: files_core::sftp_profiles::Criteria {
                        fields: [
                            format!("host{id}"),
                            String::new(),
                            String::new(),
                            String::new(),
                        ],
                    },
                })
                .collect(),
        });
        m.sftp.form.as_mut().unwrap().profile_selected = 31;
        for (w, h) in [(440, 440), (800, 440), (1000, 650)] {
            let mut p = Recorder::default();
            paint(&mut p, &mut m, w, h);
            assert!(m.hits.len() >= 10 && m.hits.len() <= 16);
            assert!(m.hits.iter().all(|hit| hit.rect.x >= 12
                && hit.rect.x + hit.rect.w <= w - 12
                && hit.rect.y >= 66
                && hit.rect.y + hit.rect.h <= h - 40));
            assert!(m.hits.iter().any(|hit| matches!(
                hit.action,
                Action::SftpCommand(_, crate::sftp_ui::Command::ProfileSelect(32))
            )));
            assert!(
                m.sftp
                    .form
                    .as_ref()
                    .unwrap()
                    .profile_geometry
                    .position(0, m.sftp.form.as_ref().unwrap().edit_epoch)
                    .is_some()
            );
        }
    }
    #[test]
    fn size_panel_virtual_rows_and_controls_stay_inside_minimum_window() {
        let mut m = Model::new("/fixture-size-panel".into());
        m.loaded = true;
        let names = (0..128)
            .map(|n| std::ffi::OsString::from(format!("folder{n}")))
            .collect::<Vec<_>>();
        m.sizes
            .enqueue(&m.path, names.iter().map(|n| n.as_os_str()))
            .unwrap();
        m.action(Action::Sizes);
        for (w, h) in [(440, 440), (800, 440), (1000, 650)] {
            let mut p = Recorder::default();
            paint(&mut p, &mut m, w, h);
            assert!(m.hits.len() < 25);
            assert!(
                m.hits
                    .iter()
                    .all(|h| matches!(h.action, Action::SizePanel(_, _)))
            );
            assert!(m.hits.iter().all(|hit| hit.rect.x >= 12
                && hit.rect.x + hit.rect.w <= w - 12
                && hit.rect.y >= 66
                && hit.rect.y + hit.rect.h <= h - 40));
        }
    }
    #[test]
    fn minimum_footer_keeps_status_and_self_pss_separate() {
        let mut m = Model::new("/fixture-only-footer".into());
        let mut p = Recorder::default();
        status(&mut p, &mut m, 440, 440);
        let pss = p
            .text
            .iter()
            .find(|(_, text)| text.starts_with("PSS"))
            .unwrap()
            .0;
        let first = p.text.first().unwrap().0;
        assert!(first.x + first.w <= pss.x);
        assert!(!p.text.iter().any(|(_, text)| text.ends_with("libres")));
    }
    #[test]
    fn size_status_displaces_help_and_free_space_without_overlapping_pss() {
        let mut m = Model::new("/fixture-only-size-footer".into());
        m.loaded = true;
        m.sizes
            .enqueue(&m.path, [std::ffi::OsStr::new("dir")].into_iter())
            .unwrap();
        m.sizes.pause(true);
        for w in [440, 800, 1000] {
            assert!(!tips_available(&m, w));
            let mut p = Recorder::default();
            status(&mut p, &mut m, w, 440);
            let (rect, text) = p.text[0].clone();
            assert!(text.contains("pausa"));
            assert!(p.text_width(&text, 12, false) <= rect.w as f32);
            let pss = p.text.iter().find(|(_, t)| t.starts_with("PSS")).unwrap().0;
            assert!(rect.x + rect.w <= pss.x);
            assert!(!p.text.iter().any(|(_, text)| text.ends_with("libres")));
        }
    }
    #[test]
    fn status_tip_ellipsis_fits_and_keeps_utf8_boundaries() {
        let mut p = Recorder::default();
        let text = "Preferencias (Ctrl+,) · Acción";
        let shortened = tip_at_rest(&mut p, text, 90);
        assert!(shortened.ends_with('…'));
        assert!(p.text_width(&shortened, 11, true) <= 90.0);
        assert_eq!(tip_at_rest(&mut p, text, 1000), text);
        assert_eq!(tip_at_rest(&mut p, text, 1), "");
    }
    #[test]
    fn short_inspector_metadata_and_directory_controls_fit_before_actions() {
        for h in [440, 500, 650] {
            for directory in [false, true] {
                let mut m = Model::new("/fixture-only-inspector".into());
                m.entries = vec![files_core::listing::Entry {
                    name: "sample.txt".into(),
                    kind: if directory {
                        files_core::listing::Kind::Directory
                    } else {
                        files_core::listing::Kind::File
                    },
                    metadata: Some(files_core::listing::Metadata {
                        size: 1234,
                        modified: SystemTime::now(),
                    }),
                    stat_attempted: true,
                }];
                m.selected = Some(0);
                let mut p = Recorder::default();
                inspector(&mut p, &mut m, 800, h, 240);
                let rename = m
                    .hits
                    .iter()
                    .find(|hit| matches!(hit.action, Action::Rename))
                    .unwrap()
                    .rect;
                assert_eq!(
                    m.hits
                        .iter()
                        .filter(|hit| matches!(hit.action, Action::CalculateSize))
                        .count(),
                    usize::from(directory && h == 650)
                );
                for hit in m
                    .hits
                    .iter()
                    .filter(|hit| matches!(hit.action, Action::CalculateSize))
                {
                    assert!(hit.rect.y >= 98 && hit.rect.y + hit.rect.h <= h - 174);
                    assert!(hit.rect.y + hit.rect.h <= rename.y);
                }
                for (area, text) in &p.text {
                    if text.starts_with("Tamaño")
                        || text.starts_with("hace")
                        || text.starts_with("Aparente")
                    {
                        assert!(
                            area.y + area.h <= h - 174,
                            "{h}px: metadata overlaps actions: {text}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn every_action_including_terminal_and_paste_fits_short_and_wide_windows() {
        for (w, h) in [(440, 440), (800, 440), (1000, 560), (1000, 650)] {
            let mut m = Model::new("/fixture-action-menu".into());
            m.loaded = true;
            m.actions_menu = true;
            m.toolkit_popup = false;
            m.operation_bar = false;
            let mut p = Recorder::default();
            paint(&mut p, &mut m, w, h);
            let (index, panel) = m
                .hits
                .iter()
                .enumerate()
                .find(|(_, hit)| matches!(hit.action, Action::None) && hit.rect.y == 58)
                .map(|(i, h)| (i, h.rect))
                .unwrap();
            assert!(panel.x >= 0 && panel.x + panel.w <= w && panel.y + panel.h <= h - 32);
            let buttons = m.hits[index + 1..]
                .iter()
                .filter(|hit| {
                    hit.rect.y >= panel.y + 36
                        && hit.rect.y + hit.rect.h <= panel.y + panel.h
                        && hit.rect.x >= panel.x
                        && hit.rect.x + hit.rect.w <= panel.x + panel.w
                })
                .collect::<Vec<_>>();
            assert_eq!(buttons.len(), 23);
            assert!(
                buttons
                    .iter()
                    .any(|h| matches!(h.action, Action::Checksums))
            );
            assert!(buttons.iter().any(|h| matches!(h.action, Action::Terminal)));
            assert!(
                buttons
                    .iter()
                    .any(|h| matches!(h.action, Action::ClipboardPaste))
            );
            if w == 440 {
                let mut measure = Recorder::default();
                for (area, text) in &p.text {
                    if area.y >= panel.y + 36
                        && area.y + area.h <= panel.y + panel.h
                        && area.x >= panel.x
                        && area.x + area.w <= panel.x + panel.w
                    {
                        assert!(
                            measure.text_width(text, 12, false) <= area.w as f32,
                            "compact label clipped: {text}"
                        );
                    }
                }
            }
            for (i, a) in buttons.iter().enumerate() {
                assert!(a.rect.h >= 20);
                for b in buttons.iter().skip(i + 1) {
                    assert!(
                        a.rect.x + a.rect.w <= b.rect.x
                            || b.rect.x + b.rect.w <= a.rect.x
                            || a.rect.y + a.rect.h <= b.rect.y
                            || b.rect.y + b.rect.h <= a.rect.y
                    );
                }
            }
        }
    }
}
