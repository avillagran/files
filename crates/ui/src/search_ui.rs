//! Search modal with a captured root and raw result paths. No filesystem I/O here.
use crate::{
    model::{Action, Hit, Model},
    painter::{Painter, Rect},
};
use fltk::{
    app,
    enums::{Event, Key, Shortcut},
};
const ROW: i32 = 46;
fn row_height(m: &Model) -> i32 {
    if m.search.hits().iter().any(|h| h.snippet.is_some()) {
        66
    } else {
        ROW
    }
}
fn paint_snippet(
    p: &mut dyn Painter,
    snippet: &files_core::search_snippet::Snippet,
    area: Rect,
    foreground: files_core::theme::Color,
    highlight: files_core::theme::Color,
    marked_foreground: files_core::theme::Color,
) {
    if !snippet.valid() || area.w < 20 {
        return;
    }
    let mut start = 0;
    if p.text_width(&snippet.text[..snippet.start], 11, true) > area.w as f32 / 3. {
        start = snippet.text[..snippet.start]
            .char_indices()
            .rev()
            .nth(11)
            .map_or(snippet.start, |(i, _)| i);
        if p.text_width(&snippet.text[start..snippet.start], 11, true) > area.w as f32 / 3. {
            start = snippet.start;
        }
    }
    p.clip(area);
    let ellipsis = p.text_width("…", 11, true).ceil() as i32;
    let edge = if start > 0 || snippet.leading {
        ellipsis
    } else {
        0
    };
    if edge > 0 {
        p.literal_text(Rect::new(area.x, area.y, edge, area.h), "…", 11, foreground);
    }
    let text_x = area.x + edge;
    let before = p
        .text_width(&snippet.text[start..snippet.start], 11, true)
        .ceil() as i32;
    let marked = p
        .text_width(&snippet.text[snippet.start..snippet.end], 11, true)
        .ceil() as i32;
    let trailing = snippet.trailing || snippet.match_clipped;
    let tail = if trailing { ellipsis } else { 0 };
    let main_width = (area.w - edge - tail).max(0);
    p.clip(Rect::new(text_x, area.y, main_width, area.h));
    p.literal_text(
        Rect::new(text_x, area.y, main_width, area.h),
        &snippet.text[start..],
        11,
        foreground,
    );
    let x = text_x + before;
    let width = marked.min(text_x + main_width - x).max(0);
    if width > 0 {
        p.rounded_rect(Rect::new(x, area.y, width, area.h), 2, highlight);
        p.literal_text(
            Rect::new(x, area.y, width, area.h),
            &snippet.text[snippet.start..snippet.end],
            11,
            marked_foreground,
        );
    }
    p.unclip();
    if trailing {
        let text_end = text_x + p.text_width(&snippet.text[start..], 11, true).ceil() as i32;
        p.literal_text(
            Rect::new(text_end.min(area.x + area.w - tail), area.y, tail, area.h),
            "…",
            11,
            foreground,
        );
    }
    p.unclip();
}

impl Model {
    pub fn search_action(&mut self, action: Action) {
        match action {
            Action::Search => {
                if self.rename_dialog.is_some()
                    || self.copy_form.is_some()
                    || self.mutation_applied
                    || self.properties_dialog.is_some()
                {
                    return;
                }
                self.preview.close();
                self.space_map.close();
                self.thumbnails.suspend();
                if let Some(job) = &self.du_job {
                    job.cancel();
                }
                self.sizes.cancel_all();
                self.du_requested = None;
                self.du_target = None;
                self.du_text = None;
                self.help = None;
                self.settings = false;
                self.actions_menu = false;
                self.end_path_edit();
                self.search
                    .open(self.path.clone(), self.query.clone(), self.show_hidden);
                self.hits.clear();
            }
            Action::SearchTransfer(moving) => self.begin_search_transfer(moving),
            Action::SearchScopeMenu => {
                if self.search.date_busy() {
                    self.search.error = "Espera la fecha pendiente antes de cambiar raíz".into();
                    return;
                }
                self.search.pointer(0, 0, false, false, false, false);
                if self.search.calendar_open {
                    self.search.toggle_calendar();
                }
                self.search.scope_menu = !self.search.scope_menu;
                self.search.scope_choice = 0;
                self.search.scope_result = Some((self.search.selected, self.search.result_epoch));
                self.text_paste.invalidate();
                self.hits.clear();
            }
            Action::SearchScope(scope, epoch) => {
                self.search.change_scope(scope, epoch);
                self.text_paste.invalidate();
                self.hits.clear();
            }
            Action::SearchStart => self.search.start(),
            Action::SearchPages => self.search.start_pages(),
            Action::SearchPageStep(delta, epoch) => self.search.page_step(delta, epoch),
            Action::SearchCancel => self.search.cancel_visible(),
            Action::SearchClose => self.search.close(),
            Action::SearchHidden => {
                self.search.cancel();
                self.search.hidden = !self.search.hidden;
                self.search.marks = Default::default();
                self.search.report = None;
                self.search.status = "Alcance cambiado · Enter vuelve a buscar";
            }
            Action::SearchFocus => self.search.focus(self.search.primary()),
            Action::SearchOrder => {
                self.search.cycle_order();
                self.hits.clear();
            }
            Action::SearchFilters => self.search.toggle_filters(),
            Action::SearchCalendar => {
                self.search.toggle_calendar();
                self.text_paste.invalidate();
                self.hits.clear();
            }
            Action::SearchDate(preset) => {
                self.search.request_date(preset);
                self.hits.clear();
            }
            Action::SearchContent => self.search.toggle_content(),
            Action::SearchRegex => self.search.toggle_regex(),
            Action::SearchField(i) => self.search.focus(i),
            Action::SearchClearFilters => self.search.clear_filters(),
            Action::SearchSelect(i, epoch) if epoch == self.search.result_epoch => {
                self.search.select_result(i, false, false, false)
            }
            Action::SearchToggle(i, epoch) if epoch == self.search.result_epoch => {
                self.search.select_result(i, true, false, false)
            }
            Action::SearchMarkAll(clear) => {
                if clear {
                    self.search.marks.clear();
                    self.search.background_tab = None;
                    self.search.error.clear();
                } else {
                    self.search.mark_all();
                }
            }
            Action::SearchBackgroundTab => {
                self.search.background_tab = None;
                self.search.background_count = 0;
                self.search.sync_order();
                let indices = self
                    .search
                    .order
                    .visible(self.search.hits())
                    .iter()
                    .copied()
                    .filter(|&i| {
                        if self.search.marks.explicit {
                            self.search
                                .hits()
                                .get(i)
                                .is_some_and(|h| self.search.marks.contains(&h.relative))
                        } else {
                            i == self.search.selected
                        }
                    })
                    .collect::<Vec<_>>();
                if indices.is_empty() {
                    self.search.error = "No quedan resultados marcados; vuelve a elegir".into();
                    return;
                }
                if indices.len() > crate::tabs::MAX_TABS.saturating_sub(self.tabs.rows.len()) {
                    self.search.error="Máximo ocho pestañas; el lote no cabe, reduce las marcas o cierra pestañas".into();
                    return;
                }
                let targets = indices
                    .iter()
                    .map(|&i| {
                        let hit = &self.search.hits()[i];
                        let path = self.search.root.join(&hit.relative);
                        if hit.kind == files_core::listing::Kind::Directory {
                            (path, None)
                        } else {
                            (
                                path.parent().unwrap_or(&self.search.root).to_path_buf(),
                                path.file_name().map(|n| n.to_os_string()),
                            )
                        }
                    })
                    .collect::<Vec<_>>();
                match self.background_tabs(targets.iter().map(|(p, n)| (p.as_path(), n.as_deref())))
                {
                    Ok(ids) => {
                        self.search.background_tab = ids.last().copied();
                        self.search.background_count = ids.len();
                        self.search.error.clear();
                    }
                    Err(e) => {
                        self.search.background_tab = None;
                        self.search.background_count = 0;
                        self.search.error = e.into();
                    }
                }
                self.hits.clear();
            }
            Action::SearchOpen(locate) => {
                let Some(hit) = self.search.hits().get(self.search.selected) else {
                    return;
                };
                let path = self.search.root.join(&hit.relative);
                let directory = hit.kind == files_core::listing::Kind::Directory;
                // Result is an observation, not authority to mutate. The normal
                // browser/open worker performs whatever later validation it needs.
                if locate {
                    let Some(parent) = path.parent() else {
                        return;
                    };
                    let parent = parent.to_path_buf();
                    let Some(name) = path.file_name().map(|n| n.to_os_string()) else {
                        return;
                    };
                    self.search.close();
                    self.navigate(parent.clone());
                    if self.path == parent {
                        self.query.clear();
                        self.rebuild();
                        self.search_locate = Some((self.generation, name));
                    }
                } else if directory {
                    self.search.close();
                    self.navigate(path);
                } else if self.open_job.is_some() {
                    self.search.error = "Ya hay una apertura pendiente".into();
                } else {
                    match crate::open_job::Job::start(path) {
                        Ok(job) => self.open_job = Some(job),
                        Err(e) => self.search.error = e,
                    }
                }
            }
            _ => {}
        }
    }
}
fn button(p: &mut dyn Painter, m: &mut Model, r: Rect, label: &str, action: Action) {
    p.rounded_rect(r, 5 * m.radius, m.palette.lighter_background);
    p.text(r, label, 12, m.palette.accent, true, false);
    m.hits.push(Hit {
        rect: r,
        action,
        generation: m.generation,
    });
}
fn result_button(
    p: &mut dyn Painter,
    m: &mut Model,
    r: Rect,
    label: &str,
    action: Action,
    enabled: bool,
) {
    if enabled {
        button(p, m, r, label, action);
    } else {
        p.rounded_rect(r, 5 * m.radius, m.palette.background);
        p.text(r, label, 12, m.palette.dark_foreground, true, false);
    }
}
fn compact_filters(w: i32, h: i32) -> bool {
    w < 800 && panel(w, h).h < 354
}
fn filter_height(open: bool, w: i32, h: i32) -> i32 {
    if open {
        if w >= 800 {
            54
        } else if compact_filters(w, h) {
            84
        } else {
            104
        }
    } else {
        0
    }
}
fn filter_field(w: i32, h: i32, index: usize) -> (Rect, Rect) {
    let r = panel(w, h);
    let columns = if w >= 800 { 4 } else { 2 };
    let width = (r.w - 28 - (columns - 1) * 8) / columns;
    let compact = compact_filters(w, h);
    let cell = if compact { 40 } else { 50 };
    let label = if compact { 14 } else { 18 };
    let input = if compact { 24 } else { 28 };
    let x = r.x + 14 + (index as i32 % columns) * (width + 8);
    let y = r.y + 150 + (index as i32 / columns) * cell;
    (
        Rect::new(x, y, width, label),
        Rect::new(x, y + label, width, input),
    )
}
fn compact_content_label(report: &files_core::search::Report) -> String {
    let c = report.content.unwrap_or_default();
    format!(
        "{} · {} · L{} B{} Δ{} E{} M{}",
        if report.complete {
            "UTF8"
        } else {
            "PARCIAL UTF8"
        },
        report.hits.len(),
        c.limited,
        c.binary,
        c.changed,
        report.errors,
        report.mounts_excluded
    )
}
fn content_stats_height(m: &Model, w: i32, h: i32) -> i32 {
    let available = panel(w, h).h - 258 - filter_height(m.search.filters_open, w, h);
    if m.search
        .report
        .as_ref()
        .is_some_and(|r| r.content.is_some())
        && available >= 44 + row_height(m)
    {
        44
    } else {
        0
    }
}
fn panel(w: i32, h: i32) -> Rect {
    Rect::new(12, 66, (w - 24).max(1), (h - 106).max(1))
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, w: i32, h: i32) {
    if !m.search.visible
        || m.saved_searches.panel.is_some()
        || m.copy_form.is_some()
        || m.notice_open
    {
        return;
    }
    m.hits.clear();
    m.search.sync_order();
    let r = panel(w, h);
    let c = m.palette.clone();
    p.rounded_rect(r, 8 * m.radius, c.darker_background);
    p.clip(r);
    p.text(
        Rect::new(r.x + 14, r.y + 8, r.w - 326, 28),
        if r.w < 600 {
            " Buscar"
        } else {
            " Buscar en subcarpetas"
        },
        16,
        c.foreground,
        true,
        false,
    );
    button(
        p,
        m,
        Rect::new(r.x + r.w - 298, r.y + 8, 64, 28),
        " F8",
        Action::SearchCalendar,
    );
    button(
        p,
        m,
        Rect::new(r.x + r.w - 224, r.y + 8, 100, 28),
        &format!(" {}", m.search.order.order.label()),
        Action::SearchOrder,
    );
    button(
        p,
        m,
        Rect::new(r.x + r.w - 114, r.y + 8, 100, 28),
        "  Esc",
        Action::SearchClose,
    );
    p.text(
        Rect::new(
            r.x + 14,
            r.y + 40,
            r.w - if m.search.page_mode { 268 } else { 204 }
                - if m.search.content_mode { 100 } else { 0 },
            24,
        ),
        &format!(
            " Raíz ▾ · {:?} · {:?}",
            m.search
                .root
                .file_name()
                .unwrap_or(m.search.root.as_os_str()),
            m.search.root
        ),
        11,
        c.dark_foreground,
        true,
        false,
    );
    if m.search.content_mode {
        p.rect(
            Rect::new(
                r.x + 14,
                r.y + 40,
                r.w - if m.search.page_mode { 268 } else { 204 }
                    - if m.search.content_mode { 100 } else { 0 },
                24,
            ),
            c.darker_background,
        );
        p.text(
            Rect::new(
                r.x + 14,
                r.y + 40,
                r.w - if m.search.page_mode { 268 } else { 204 }
                    - if m.search.content_mode { 100 } else { 0 },
                24,
            ),
            &format!(
                " Raíz ▾ · {:?} · nombre: {} · Ctrl+F6 literal/regex",
                m.search
                    .root
                    .file_name()
                    .unwrap_or(m.search.root.as_os_str()),
                if m.search.query.is_empty() {
                    "(todos)"
                } else {
                    &m.search.query
                }
            ),
            11,
            c.accent,
            true,
            false,
        );
    }
    m.hits.push(Hit {
        rect: Rect::new(
            r.x + 14,
            r.y + 40,
            r.w - if m.search.page_mode { 268 } else { 204 }
                - if m.search.content_mode { 100 } else { 0 },
            24,
        ),
        action: Action::SearchScopeMenu,
        generation: m.generation,
    });
    if m.search.content_mode {
        button(
            p,
            m,
            Rect::new(
                r.x + r.w - if m.search.page_mode { 352 } else { 288 },
                r.y + 40,
                92,
                24,
            ),
            if m.search.content_regex {
                "Regex ✓"
            } else {
                "Literal"
            },
            Action::SearchRegex,
        );
    }
    button(
        p,
        m,
        Rect::new(r.x + r.w - 180, r.y + 40, 64, 24),
        " F9",
        Action::SearchPages,
    );
    if m.search.page_mode {
        for (i, delta, label, enabled) in [
            (0, -1, "", m.search.pages.previous()),
            (
                1,
                1,
                "",
                m.search.pages.next_cached() || m.search.pages.can_request(),
            ),
        ] {
            let area = Rect::new(r.x + r.w - 252 + i * 36, r.y + 40, 28, 24);
            if enabled {
                button(
                    p,
                    m,
                    area,
                    label,
                    Action::SearchPageStep(delta, m.search.result_epoch),
                );
            } else {
                p.text(area, label, 12, c.dark_foreground, true, false);
            }
        }
    }
    button(
        p,
        m,
        Rect::new(r.x + r.w - 108, r.y + 40, 94, 24),
        " F2",
        Action::SavedSearches(false),
    );
    let field = Rect::new(r.x + 14, r.y + 72, r.w - 232, 34);
    p.rounded_rect(field, 4 * m.radius, c.background);
    m.search.paint_input(p, m.search.primary(), field, &c);
    m.hits.push(Hit {
        rect: field,
        action: Action::SearchFocus,
        generation: m.generation,
    });
    button(
        p,
        m,
        Rect::new(r.x + r.w - 204, r.y + 72, 90, 34),
        if m.search.calendar_open {
            " Revisar"
        } else if m.search.searching_current() {
            " Reiniciar"
        } else {
            " Buscar"
        },
        if m.search.calendar_open {
            Action::SearchCalendar
        } else {
            Action::SearchStart
        },
    );
    result_button(
        p,
        m,
        Rect::new(r.x + r.w - 106, r.y + 72, 92, 34),
        " Parar",
        Action::SearchCancel,
        m.search.running(),
    );
    let hidden = if m.search.hidden {
        "  Ocultos: sí"
    } else {
        "  Ocultos: no"
    };
    button(
        p,
        m,
        Rect::new(r.x + 14, r.y + 116, 148, 26),
        hidden,
        Action::SearchHidden,
    );
    let count = m.search.fields.iter().filter(|s| !s.is_empty()).count();
    button(
        p,
        m,
        Rect::new(r.x + 174, r.y + 116, 132, 26),
        &format!(" Filtros:{count} F4"),
        Action::SearchFilters,
    );
    if m.search.filters_open {
        button(
            p,
            m,
            Rect::new(r.x + r.w - 108, r.y + 116, 94, 26),
            " Quitar",
            Action::SearchClearFilters,
        );
    } else {
        button(
            p,
            m,
            Rect::new(r.x + r.w - 108, r.y + 116, 94, 26),
            if m.search.content_mode {
                " Txt✓ F6"
            } else if !m.search.content.is_empty() {
                " Txt+ F6"
            } else {
                " Texto F6"
            },
            Action::SearchContent,
        );
    }
    if m.search.calendar_open {
        use files_core::search_dates::Preset;
        let bw = (r.w - 44) / 3;
        for (i, (label, preset)) in [
            (" Hoy · 1", Preset::Today),
            (" Ayer · 2", Preset::Yesterday),
            (" 7 días · 3", Preset::LastSevenDays),
            (" 1 h · 4", Preset::LastHour),
            (" 24 h · 5", Preset::Last24Hours),
            (" 168 h · 6", Preset::Last168Hours),
        ]
        .into_iter()
        .enumerate()
        {
            button(
                p,
                m,
                Rect::new(
                    r.x + 14 + (i % 3) as i32 * (bw + 8),
                    r.y + 150 + (i / 3) as i32 * 36,
                    bw,
                    28,
                ),
                label,
                Action::SearchDate(preset),
            );
        }
        p.text(Rect::new(r.x+14,r.y+222,r.w-28,36),"1–3: fechas locales · 4–6: horas transcurridas.\nUTC fijado al elegir; segundos completos.",11,c.foreground,true,false);
        p.text(
            Rect::new(r.x + 14, r.y + r.h - 68, r.w - 28, 26),
            if m.search.error.is_empty() {
                m.search.status
            } else {
                &m.search.error
            },
            11,
            if m.search.error.is_empty() {
                c.foreground
            } else {
                c.red
            },
            true,
            false,
        );
        button(
            p,
            m,
            Rect::new(r.x + 14, r.y + r.h - 36, r.w - 28, 28),
            " Volver a criterios · F8 / Enter / Esc",
            Action::SearchCalendar,
        );
        p.unclip();
        return;
    }
    let extra = if m.search.filters_open {
        for i in 0..4 {
            let (label_area, field) = filter_field(w, h, i);
            let labels = [
                "Mínimo (10MiB)",
                "Máximo (50MiB)",
                "Desde (UTC)",
                "Antes de (UTC)",
            ];
            p.text(label_area, labels[i], 11, c.dark_foreground, true, false);
            p.rounded_rect(field, 4 * m.radius, c.background);
            m.search.paint_input(p, i + 1, field, &c);
            if m.search.fields[i].is_empty() && !(m.search.field == i + 1 && m.search.editing_input)
            {
                p.text(
                    Rect::new(field.x + 8, field.y, field.w - 16, field.h),
                    if i < 2 { "Sin límite" } else { "AAAA-MM-DD" },
                    12,
                    c.dark_foreground,
                    true,
                    false,
                );
            }
            m.hits.push(Hit {
                rect: field,
                action: Action::SearchField(i + 1),
                generation: m.generation,
            });
        }
        filter_height(true, w, h)
    } else {
        0
    };
    let mut body = Rect::new(
        r.x + 14,
        r.y + 150 + extra,
        r.w - 28,
        (r.h - 258 - extra).max(0),
    );
    let stats_height = content_stats_height(m, w, h);
    if let Some(stats) = m
        .search
        .report
        .as_ref()
        .and_then(|r| r.content)
        .filter(|_| stats_height > 0)
    {
        if body.h >= 44 {
            p.text(
                Rect::new(body.x, body.y, body.w, 20),
                "UTF8 · fragmentos256B · 1MiB/archivo · 64MiB",
                11,
                c.dark_foreground,
                true,
                false,
            );
            p.text(
                Rect::new(body.x, body.y + 20, body.w, 20),
                &format!(
                    "Archivos {} · bin {} · límite {} · cambio {}",
                    stats.files, stats.binary, stats.limited, stats.changed
                ),
                11,
                c.foreground,
                true,
                false,
            );
        }
        body.y += stats_height;
        body.h = (body.h - stats_height).max(0);
    }
    let row_height = row_height(m);
    let page = (body.h / row_height).max(1) as usize;
    let selected_position = m
        .search
        .order
        .visible(m.search.hits())
        .iter()
        .position(|&i| i == m.search.selected)
        .unwrap_or(0);
    if selected_position < m.search.scroll {
        m.search.scroll = selected_position;
    }
    if selected_position >= m.search.scroll + page {
        m.search.scroll = selected_position + 1 - page;
    }
    p.clip(body);
    if (m.search.report.is_some() || m.search.provisional.is_some()) && body.h >= row_height {
        for (position, &index) in m
            .search
            .order
            .visible(m.search.hits())
            .iter()
            .enumerate()
            .skip(m.search.scroll)
            .take(page)
        {
            let hit = &m.search.hits()[index];
            let y = body.y + (position - m.search.scroll) as i32 * row_height;
            let row = Rect::new(body.x, y, body.w, row_height - 2);
            if m.search.marks.contains(&hit.relative) {
                p.rounded_rect(row, 4 * m.radius, c.selection);
            }
            if index == m.search.selected {
                p.rounded_rect(row, 4 * m.radius, c.selection);
                p.rect(Rect::new(row.x, row.y, 2, row.h), c.accent);
            }
            let name = files_core::name_label::label(
                hit.relative.file_name().unwrap_or(hit.relative.as_os_str()),
            );
            let glyph = if hit.kind == files_core::listing::Kind::Directory {
                ""
            } else if hit.kind == files_core::listing::Kind::Symlink {
                ""
            } else {
                ""
            };
            p.text(
                Rect::new(
                    row.x + 8,
                    y,
                    row.w - if hit.content.is_some() { 96 } else { 16 },
                    23,
                ),
                &format!(
                    "{} {glyph} {name}",
                    if m.search.marks.contains(&hit.relative) {
                        ""
                    } else {
                        ""
                    }
                ),
                12,
                c.foreground,
                true,
                false,
            );
            if let Some(found) = hit.content {
                p.text(
                    Rect::new(row.x + row.w - 84, y, 76, 23),
                    &format!(
                        "L{}{}",
                        found.line,
                        if found.zero_width() { " · ∅" } else { "" }
                    ),
                    11,
                    c.accent,
                    true,
                    true,
                );
            }
            p.text(
                Rect::new(row.x + 32, y + 23, row.w - 40, 20),
                &files_core::name_label::label(
                    hit.relative
                        .parent()
                        .unwrap_or(std::path::Path::new("."))
                        .as_os_str(),
                ),
                11,
                c.dark_foreground,
                true,
                false,
            );
            if let Some(snippet) = &hit.snippet {
                paint_snippet(
                    p,
                    snippet,
                    Rect::new(row.x + 8, y + 44, row.w - 16, 18),
                    c.foreground,
                    c.accent,
                    c.darker_background,
                );
            }
            m.hits.push(Hit {
                rect: row,
                action: Action::SearchSelect(index, m.search.result_epoch),
                generation: m.generation,
            });
            m.hits.push(Hit {
                rect: Rect::new(row.x + 4, row.y, 24, 23),
                action: Action::SearchToggle(index, m.search.result_epoch),
                generation: m.generation,
            });
        }
        if m.search.hits().is_empty() {
            p.text(
                body,
                if m.search.report.as_ref().is_some_and(|r| r.complete) {
                    "Sin coincidencias en este alcance"
                } else {
                    "Sin coincidencias en la parte recorrida"
                },
                13,
                c.foreground,
                true,
                false,
            );
        }
    } else if body.h >= 54 && m.search.searching_current() {
        p.text(body,"Buscando en subcarpetas…\nLas coincidencias aparecerán durante el recorrido.\nParar cancela el recorrido.",12,c.dark_foreground,true,false);
    } else if body.h >= 54 {
        p.text(body,"*.pdf · report 2026 !draft · ext:rs,md · type:dir\nsize:>10MiB · after:2026-01-01 · before:2027-01-01 (UTC)\ncontent:\"texto literal\" · regex:\"(?m)^error[0-9]+\"",12,c.dark_foreground,true,false);
    }
    p.unclip();
    let label = if !m.search.error.is_empty() {
        m.search.error.clone()
    } else if let Some(report) = &m.search.report {
        if report.content.is_some() && content_stats_height(m, w, h) == 0 {
            compact_content_label(report)
        } else {
            format!(
                "{} · {} · {} resultados · {} visitadas · {} errores · {} montajes excluidos",
                report.reason.as_deref().unwrap_or("Completa"),
                if report.content.is_some() {
                    "UTF8"
                } else {
                    "Nombres"
                },
                report.hits.len(),
                report.visited,
                report.errors,
                report.mounts_excluded
            )
        }
    } else if m.search.provisional.is_some() {
        format!(
            "PROVISIONAL · visibles: {} · en curso",
            m.search.hits().len()
        )
    } else if let Some(progress) = m.search.progress {
        format!(
            "Buscando · {} entradas · {} posibles · {} errores",
            progress.visited, progress.found, progress.errors
        )
    } else {
        m.search.status.into()
    };
    let label = if !m.search.page_mode
        && m.search.error.is_empty()
        && m.search.report.as_ref().is_some_and(|r| {
            r.reason
                .as_ref()
                .is_some_and(|s| s.starts_with("Límite de resultados"))
        }) {
        format!("500: F9 busca por páginas · {label}")
    } else {
        label
    };
    let label =
        if m.search.page_mode && m.search.pages.current.is_some() && m.search.error.is_empty() {
            let state = if m.search.pages.cancelled {
                "cancelado"
            } else if m.search.pages.loading {
                "espera"
            } else if m.search.pages.ended && m.search.pages.complete {
                "fin"
            } else if m.search.pages.ended {
                "parcial"
            } else {
                "seguir"
            };
            if let Some(content) = m.search.report.as_ref().and_then(|r| r.content) {
                format!(
                    "P{} · {}/Σ{} · h{}–{} {state} · L{} B{} Δ{} E{} M{}",
                    m.search.pages.current.map_or(0, |p| p.sequence + 1),
                    m.search.hits().len(),
                    m.search.pages.found,
                    m.search.pages.oldest().map_or(0, |n| n + 1),
                    m.search.pages.latest().map_or(0, |n| n + 1),
                    content.limited,
                    content.binary,
                    content.changed,
                    m.search.pages.errors,
                    m.search.pages.mounts
                )
            } else {
                format!(
                    "P{} · {} filas · Σ{} · historial{}–{} · {}{}",
                    m.search.pages.current.map_or(0, |m| m.sequence + 1),
                    m.search.hits().len(),
                    m.search.pages.found,
                    m.search.pages.oldest().map_or(0, |n| n + 1),
                    m.search.pages.latest().map_or(0, |n| n + 1),
                    state,
                    if m.search.pages.errors > 0 || m.search.pages.mounts > 0 {
                        format!(" E{} M{}", m.search.pages.errors, m.search.pages.mounts)
                    } else {
                        String::new()
                    }
                )
            }
        } else {
            label
        };
    let label = if m.search_transfer.busy() {
        "Capturando orígenes · Esc cancela · espera ACK".into()
    } else if m.search.marks.explicit {
        format!(
            "✓{}{} · {label}",
            m.search.marks.len(),
            if m.search.marks.pruned > 0 {
                format!(" · {} descartados", m.search.marks.pruned)
            } else {
                String::new()
            }
        )
    } else {
        label
    };
    p.text(
        Rect::new(r.x + 14, r.y + r.h - 104, r.w - 28, 26),
        &label,
        11,
        if m.search.error.is_empty() {
            c.foreground
        } else {
            c.red
        },
        true,
        false,
    );
    let usable = body.h >= row_height && m.search.hits().get(m.search.selected).is_some();
    let width = (r.w - 44) / 3;
    let compact = width < 210;
    let added = m.search.background_tab.map(|id| {
        if m.search.background_count > 1 {
            if compact {
                format!(" +{}", m.search.background_count)
            } else {
                format!(" +{} pestañas", m.search.background_count)
            }
        } else {
            if compact {
                format!(" #{id}")
            } else {
                format!(" Añadida #{id}")
            }
        }
    });
    let tab_short = added.as_deref().unwrap_or(" Pestaña");
    let tab_long = added.as_deref().unwrap_or(" Pestaña · Ctrl+Enter");
    for (i, (short, long, action)) in [
        (" Abrir", " Abrir · Enter", Action::SearchOpen(false)),
        (
            " Localizar",
            " Localizar · Alt+Enter",
            Action::SearchOpen(true),
        ),
        (tab_short, tab_long, Action::SearchBackgroundTab),
        (
            if m.search.marks.len() > 0 {
                " Ninguno"
            } else if m.search.page_mode {
                " Página"
            } else {
                " Todo"
            },
            if m.search.marks.len() > 0 {
                " Ninguno · Ctrl+D"
            } else if m.search.page_mode {
                " Marcar página · Ctrl+A"
            } else {
                " Todo · Ctrl+A"
            },
            Action::SearchMarkAll(m.search.marks.len() > 0),
        ),
        (
            " Copiar F5",
            " Copiar · F5",
            Action::SearchTransfer(false),
        ),
        (
            " Mover ⇧F6",
            " Mover · Shift+F6",
            Action::SearchTransfer(true),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        result_button(
            p,
            m,
            Rect::new(
                r.x + 14 + (i % 3) as i32 * (width + 8),
                r.y + r.h - 72 + (i / 3) as i32 * 36,
                width,
                28,
            ),
            if compact { short } else { long },
            action,
            usable && !m.search_transfer.busy(),
        );
    }
    p.unclip();
    if m.search.scope_menu {
        paint_scope(p, m, r);
    }
}
fn scope_rows(m: &Model) -> [(crate::search_state::Scope, &'static str); 3] {
    use crate::search_state::Scope;
    [
        (Scope::Parent, " Subir carpeta · Alt+↑"),
        (
            Scope::Result(if m.search.scope_menu {
                m.search.scope_result_index().unwrap_or(usize::MAX)
            } else {
                m.search.selected
            }),
            " Carpeta del resultado · Alt+→",
        ),
        (Scope::Back, " Raíz anterior · Alt+←"),
    ]
}
fn paint_scope(p: &mut dyn Painter, m: &mut Model, r: Rect) {
    m.hits.clear();
    let boxr = Rect::new(r.x + 14, r.y + 68, (r.w - 28).min(390), 156);
    p.rounded_rect(boxr, 6 * m.radius, m.palette.muted);
    for (i, (scope, label)) in scope_rows(m).into_iter().enumerate() {
        let row = Rect::new(boxr.x + 8, boxr.y + 8 + i as i32 * 36, boxr.w - 16, 32);
        let enabled = m.search.scope_target(scope).is_some();
        if i == m.search.scope_choice {
            p.rect(row, m.palette.selection);
        }
        p.literal_text(
            row,
            label,
            11,
            if enabled {
                m.palette.accent
            } else {
                m.palette.dark_foreground
            },
        );
        if enabled {
            m.hits.push(Hit {
                rect: row,
                action: Action::SearchScope(scope, m.search.scope_stamp(scope)),
                generation: m.generation,
            });
        }
    }
    button(
        p,
        m,
        Rect::new(boxr.x + 8, boxr.y + 120, boxr.w - 16, 28),
        " Volver · Esc",
        Action::SearchScopeMenu,
    );
}
pub fn event(m: &mut Model, event: Event, w: i32, h: i32) -> bool {
    match event {
        Event::Push => {
            if app::event_button() != 1 {
                return true;
            }
            if !m.search.scope_menu
                && m.search.pointer(
                    app::event_x(),
                    app::event_y(),
                    true,
                    false,
                    app::event_state().intersects(Shortcut::Shift),
                    app::event_clicks(),
                )
            {
                return true;
            }
            let action = m
                .hits
                .iter()
                .rev()
                .find(|v| {
                    v.generation == m.generation && v.rect.contains(app::event_x(), app::event_y())
                })
                .map(|v| v.action.clone());
            if action.is_none() && m.search.scope_menu {
                m.action(Action::SearchScopeMenu);
            }
            if let Some(action) = action {
                let current = match action {
                    Action::SearchSelect(_, epoch) | Action::SearchToggle(_, epoch) => {
                        epoch == m.search.result_epoch
                    }
                    _ => true,
                };
                let open =
                    current && matches!(action, Action::SearchSelect(..)) && app::event_clicks();
                match action {
                    Action::SearchSelect(i, _) | Action::SearchToggle(i, _) if current => {
                        let mods = app::event_state();
                        let ctrl = mods.intersects(Shortcut::Ctrl | Shortcut::Meta);
                        let shift = mods.intersects(Shortcut::Shift);
                        let toggle = matches!(action, Action::SearchToggle(..)) || ctrl && !shift;
                        m.search.select_result(i, toggle, shift, ctrl);
                    }
                    Action::SearchSelect(..) | Action::SearchToggle(..) => {}
                    _ => m.action(action),
                }
                if open
                    && !app::event_state()
                        .intersects(Shortcut::Ctrl | Shortcut::Meta | Shortcut::Shift)
                {
                    m.action(Action::SearchOpen(false));
                }
            }
        }
        Event::Drag if !m.search.scope_menu => {
            m.search
                .pointer(app::event_x(), app::event_y(), false, true, false, false);
        }
        Event::Released if !m.search.scope_menu => {
            m.search.pointer(0, 0, false, false, false, false);
        }
        Event::MouseWheel if !m.search.scope_menu => {
            m.search.editing_input = false;
            m.search.step(app::event_dy_value() * 3);
        }
        Event::KeyDown | Event::Shortcut => key(
            m,
            app::event_key(),
            app::event_state(),
            &app::event_text(),
            w,
            h,
        ),
        _ => {}
    }
    true
}

/// Shared keyboard policy; private fixtures call this without synthesizing OS input.
pub fn key(m: &mut Model, key: Key, modifiers: Shortcut, text: &str, w: i32, h: i32) {
    if modifiers.intersects(Shortcut::Alt) && matches!(key, Key::Up | Key::Left | Key::Right) {
        let scope = match key {
            Key::Up => crate::search_state::Scope::Parent,
            Key::Left => crate::search_state::Scope::Back,
            _ => crate::search_state::Scope::Result(m.search.selected),
        };
        m.action(Action::SearchScope(scope, m.search.scope_stamp(scope)));
        return;
    }
    if m.search.scope_menu {
        match key {
            Key::Escape => m.action(Action::SearchScopeMenu),
            Key::Up => {
                m.search.scope_choice = (m.search.scope_choice + 2) % 3;
            }
            Key::Down => {
                m.search.scope_choice = (m.search.scope_choice + 1) % 3;
            }
            Key::Enter | Key::KPEnter => {
                let scope = scope_rows(m)[m.search.scope_choice].0;
                m.action(Action::SearchScope(scope, m.search.scope_stamp(scope)));
            }
            _ => {}
        }
        return;
    }
    if key == Key::F2 {
        m.action(Action::SavedSearches(false));
        return;
    }
    if modifiers.intersects(Shortcut::Ctrl | Shortcut::Meta) && key == Key::from_char('s') {
        m.action(Action::SavedSearches(true));
        return;
    }

    let ctrl = modifiers.intersects(Shortcut::Ctrl | Shortcut::Meta);
    if key == Key::F8 {
        m.action(Action::SearchCalendar);
        return;
    }
    if m.search.calendar_open {
        use files_core::search_dates::Preset;
        let preset = match key {
            k if k == Key::from_char('1') => Some(Preset::Today),
            k if k == Key::from_char('2') => Some(Preset::Yesterday),
            k if k == Key::from_char('3') => Some(Preset::LastSevenDays),
            k if k == Key::from_char('4') => Some(Preset::LastHour),
            k if k == Key::from_char('5') => Some(Preset::Last24Hours),
            k if k == Key::from_char('6') => Some(Preset::Last168Hours),
            _ => None,
        };
        if let Some(preset) = preset {
            m.action(Action::SearchDate(preset));
        } else if matches!(key, Key::Escape | Key::Enter | Key::KPEnter) {
            m.action(Action::SearchCalendar);
        } else if key == Key::F4 {
            m.action(Action::SearchFilters);
        } else if ctrl && key == Key::from_char('f') {
            m.action(Action::SearchFocus);
        }
        return;
    }

    let shift = modifiers.intersects(Shortcut::Shift);
    if key == Key::F9 {
        m.action(Action::SearchPages);
        return;
    }
    if ctrl && matches!(key, Key::PageUp | Key::PageDown) {
        m.action(Action::SearchPageStep(
            if key == Key::PageUp { -1 } else { 1 },
            m.search.result_epoch,
        ));
        return;
    }
    if ctrl && key == Key::F6 {
        m.action(Action::SearchRegex);
    } else if !m.search.editing_input && !m.search.filters_open && key == Key::F5 {
        m.action(Action::SearchTransfer(false));
    } else if !m.search.editing_input && !m.search.filters_open && shift && key == Key::F6 {
        m.action(Action::SearchTransfer(true));
    } else if key == Key::F7 {
        m.action(Action::SearchOrder);
    } else if key == Key::F4 {
        m.action(Action::SearchFilters);
    } else if key == Key::F6 {
        m.action(Action::SearchContent);
    } else if key == Key::Tab && m.search.filters_open {
        let delta = if modifiers.intersects(Shortcut::Shift) {
            4
        } else {
            1
        };
        let fields = [m.search.primary(), 1, 2, 3, 4];
        let current = fields
            .iter()
            .position(|&i| i == m.search.field)
            .unwrap_or(0);
        m.search.focus(fields[(current + delta) % 5]);
    } else if key == Key::Tab {
        if m.search.editing_input {
            m.search.results_focus();
        } else {
            m.search.focus(m.search.primary());
        }
    } else if key == Key::Escape {
        if m.search.filters_open {
            m.action(Action::SearchFilters);
        } else {
            m.action(Action::SearchClose);
        }
    } else if ctrl
        && (key == Key::Enter || key == Key::KPEnter)
        && !m.search.filters_open
        && !m.search.editing_input
    {
        m.action(Action::SearchBackgroundTab);
    } else if key == Key::Enter || key == Key::KPEnter {
        if m.search.hits().get(m.search.selected).is_some()
            && !m.search.filters_open
            && !m.search.editing_input
        {
            m.action(Action::SearchOpen(modifiers.intersects(Shortcut::Alt)));
        } else if m.search.filters_open || !m.search.searching_current() {
            m.action(Action::SearchStart);
        }
    } else if ctrl && key == Key::from_char('f') {
        m.search.focus(m.search.primary());
    } else if ctrl && key == Key::from_char('a') {
        if m.search.editing_input {
            m.search.select_input();
        } else {
            m.search.mark_all();
        }
    } else if !m.search.editing_input && key == Key::Insert {
        m.search
            .select_result(m.search.selected, true, false, false);
        if m.search.error.is_empty() {
            m.search.step(1);
        }
    } else if !m.search.editing_input && !ctrl && key == Key::from_char(' ') {
        m.search
            .select_result(m.search.selected, true, false, false);
    } else if !m.search.editing_input && ctrl && key == Key::from_char('d') {
        m.search.marks.clear();
        m.search.background_tab = None;
        m.search.error.clear();
    } else if key == Key::Left
        || key == Key::Right
        || (m.search.editing_input && (key == Key::Home || key == Key::End))
    {
        use crate::line_edit::Motion;
        let motion = match key {
            Key::Home => Motion::Start,
            Key::End => Motion::End,
            Key::Left if ctrl => Motion::WordLeft,
            Key::Right if ctrl => Motion::WordRight,
            Key::Left => Motion::Left,
            _ => Motion::Right,
        };
        m.search.motion(motion, shift);
    } else if ctrl && (key == Key::from_char('z') || key == Key::from_char('y')) {
        m.search.undo(shift || key == Key::from_char('y'));
    } else if ctrl && (key == Key::from_char('c') || key == Key::from_char('x')) {
        let text = m.search.selected_text();
        if m.search.editing_input && !text.is_empty() {
            app::copy(text);
            if key == Key::from_char('x') {
                m.search.delete(false, false);
            }
        }
    } else if key == Key::Up
        || key == Key::Down
        || key == Key::Home
        || key == Key::End
        || key == Key::PageUp
        || key == Key::PageDown
    {
        m.search.editing_input = false;
        let page = ((panel(w, h).h
            - 258
            - filter_height(m.search.filters_open, w, h)
            - content_stats_height(m, w, h))
            / row_height(m))
        .max(1);
        m.search.step_range(
            match key {
                Key::Up => -1,
                Key::Down => 1,
                Key::Home => i32::MIN,
                Key::End => i32::MAX,
                Key::PageUp => -page,
                _ => page,
            },
            shift,
            ctrl,
        );
    } else if key == Key::BackSpace || key == Key::Delete {
        m.search.delete(key == Key::BackSpace, ctrl);
    } else if !ctrl && !text.is_empty() && text.chars().all(|c| !c.is_control()) {
        m.search.input(text);
    }
}

#[cfg(test)]
mod snippet_tests {
    use super::*;
    use files_core::theme::Color;
    #[derive(Default)]
    struct Recorder {
        rects: Vec<Rect>,
        text: Vec<String>,
    }
    impl Painter for Recorder {
        fn rect(&mut self, r: Rect, _: Color) {
            self.rects.push(r);
        }
        fn rounded_rect(&mut self, r: Rect, _: i32, _: Color) {
            self.rects.push(r);
        }
        fn text(&mut self, _: Rect, _: &str, _: i32, _: Color, _: bool, _: bool) {
            panic!("snippet must be literal");
        }
        fn literal_text(&mut self, r: Rect, t: &str, _: i32, _: Color) {
            self.rects.push(r);
            self.text.push(t.into());
        }
        fn image(&mut self, _: Rect, _: &crate::painter::Image<'_>) {}
        fn clip(&mut self, _: Rect) {}
        fn unclip(&mut self) {}
    }
    #[test]
    fn snippet_highlight_stays_visible_in_compact_unicode_context_and_is_literal() {
        let snippet = files_core::search_snippet::Snippet {
            text: format!("{}needle{}", "ñ".repeat(32), "tail".repeat(40)),
            start: 64,
            end: 70,
            leading: true,
            trailing: true,
            match_clipped: false,
        };
        for width in [100, 372, 900] {
            let mut p = Recorder::default();
            let area = Rect::new(10, 20, width, 18);
            paint_snippet(
                &mut p,
                &snippet,
                area,
                Color(255, 255, 255),
                Color(200, 100, 0),
                Color(0, 0, 0),
            );
            assert!(p.text.iter().any(|s| s == "needle"));
            assert!(p.rects.iter().all(|r| r.x >= area.x
                && r.y == area.y
                && r.x + r.w <= area.x + area.w
                && r.h == area.h));
        }
    }
    #[test]
    fn compact_content_results_keep_a_complete_row_before_extended_statistics() {
        let mut m = Model::new("/owned-layout".into());
        m.search.report = Some(files_core::search::Report {
            content: Some(Default::default()),
            hits: vec![files_core::search::Hit {
                relative: "owned.txt".into(),
                kind: files_core::listing::Kind::File,
                content: Some(files_core::search_content::Match {
                    byte: 0,
                    line: 1,
                    regex_end: None,
                }),
                snippet: Some(files_core::search_snippet::Snippet {
                    text: "owned".into(),
                    start: 0,
                    end: 1,
                    leading: false,
                    trailing: false,
                    match_clipped: false,
                }),
            }],
            ..Default::default()
        });
        assert_eq!(row_height(&m), 66);
        assert_eq!(content_stats_height(&m, 440, 440), 0);
        assert!(panel(440, 440).h - 258 - content_stats_height(&m, 440, 440) >= row_height(&m));
        let report = m.search.report.as_mut().unwrap();
        report.content.as_mut().unwrap().limited = 3;
        report.content.as_mut().unwrap().binary = 2;
        report.errors = 1;
        report.complete = false;
        let label = compact_content_label(report);
        assert!(label.contains("PARCIAL UTF8") && label.contains("L3 B2") && label.contains("E1"));
        assert_eq!(content_stats_height(&m, 1000, 650), 44);
        assert!(panel(1000, 650).h - 258 - 44 >= row_height(&m));
    }
    #[test]
    fn all_four_compact_criteria_fields_stay_above_footer_without_overlap() {
        for (w, h) in [(440, 440), (500, 440), (800, 440), (1000, 650)] {
            let r = panel(w, h);
            let footer = r.y + r.h - 104;
            for i in 0..4 {
                let (label, field) = filter_field(w, h, i);
                assert!(label.y + label.h <= field.y);
                assert!(field.x >= r.x + 14 && field.x + field.w <= r.x + r.w - 14);
                assert!(field.y + field.h <= footer, "field {i} at {w}x{h}");
                assert!(field.h >= 24);
            }
        }
    }
}

#[cfg(all(test, unix))]
mod path_label_tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;
    #[derive(Default)]
    struct Labels(Vec<String>);
    impl Painter for Labels {
        fn rect(&mut self, _: Rect, _: files_core::theme::Color) {}
        fn rounded_rect(&mut self, _: Rect, _: i32, _: files_core::theme::Color) {}
        fn text(
            &mut self,
            _: Rect,
            text: &str,
            _: i32,
            _: files_core::theme::Color,
            _: bool,
            _: bool,
        ) {
            self.0.push(text.into());
        }
        fn image(&mut self, _: Rect, _: &crate::painter::Image<'_>) {}
        fn clip(&mut self, _: Rect) {}
        fn unclip(&mut self) {}
    }
    #[test]
    fn result_paint_escapes_leaf_and_parent_but_locate_keeps_original_bytes() {
        let mut m = Model::new("/fixture-search-labels".into());
        m.loaded = true;
        m.search.visible = true;
        m.search.root = m.path.clone();
        let relative = std::path::PathBuf::from(std::ffi::OsString::from_vec(
            b"dir\xff\n/raw\xfe\t.txt".to_vec(),
        ));
        m.search.report = Some(files_core::search::Report {
            hits: vec![files_core::search::Hit {
                relative: relative.clone(),
                kind: files_core::listing::Kind::File,
                content: None,
                snippet: None,
            }],
            ..Default::default()
        });
        m.search.sync_order();
        for (w, h) in [(440, 440), (1000, 650)] {
            let mut p = Labels::default();
            paint(&mut p, &mut m, w, h);
            assert!(p.0.iter().any(|s| s.contains("raw\\xFE\\t.txt")));
            assert!(p.0.iter().any(|s| s == "dir\\xFF\\n"));
            assert_eq!(m.search.hits()[0].relative, relative);
        }
        let expected = m.search.root.join(&relative);
        m.search_action(Action::SearchOpen(true));
        assert_eq!(m.path, expected.parent().unwrap());
        assert_eq!(
            m.search_locate.as_ref().unwrap().1,
            expected.file_name().unwrap()
        );
    }
}
