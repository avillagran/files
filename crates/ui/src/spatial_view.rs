use crate::{
    model::{Action, Hit, Model, View},
    painter::{Painter, Rect},
};
use files_core::spatial::Position;
use std::sync::Arc;
#[derive(Clone)]
pub struct Drag {
    generation: u64,
    start: (i32, i32),
    positions: Vec<(usize, Position)>,
    moved: bool,
}
fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect::new(x, y, w, h)
}
impl Model {
    pub fn layout_changed(&mut self) {
        self.next_revision += 1;
        self.layout_revision = self.next_revision;
        self.layout_dirty = true;
    }
    /// Admission is enough to navigate; only an exact successful ACK clears dirty.
    pub fn save_layout(&mut self) -> bool {
        if self.layout_dirty && self.layout_ready {
            let Some(path) = self.layout_path.clone() else {
                return false;
            };
            if self.saves.latest_revision(&path) != Some(self.layout_revision)
                && let Err(rejected) = self.saves.submit_shared(
                    path.clone(),
                    self.layout_revision,
                    Arc::clone(&self.layout),
                )
            {
                debug_assert_eq!(rejected.directory, path);
                debug_assert_eq!(rejected.revision, self.layout_revision);
                debug_assert!(Arc::ptr_eq(&rejected.layout, &self.layout));
                self.error = format!(
                    "Disposición sin guardar: {}. Espere o reintente antes de navegar.",
                    rejected.reason
                );
                return false;
            }
        }
        self.saves.dispatch(&self.aux.tx);
        true
    }
    pub fn retry_saves(&mut self) {
        if self.sftp_profiles.needs_resolution() {
            self.sftp_profiles.refresh();
        }
        if self.saved_searches.needs_resolution() {
            self.saved_searches.refresh();
        }
        if self.folder_views.needs_resolution() {
            self.folder_views.refresh();
        }
        if self.bookmarks.needs_resolution() {
            self.bookmarks.refresh();
        }
        for path in self.saves.failed_directories() {
            self.saves.retry(&path);
        }
        self.config_error = None;
        let retry_session = self.session.error.is_some();
        self.session.retry();
        if retry_session && self.session.pending.is_none() && !self.closing {
            self.request_session_save();
        }
        self.session.dispatch(&self.aux.tx);
        self.dispatch_config();
        self.save_layout();
    }
    pub fn close_ready(&mut self) -> bool {
        self.listing_sync.stop();
        // Signal ALL subsystems first. A slow PrepareMove must not postpone
        // cancellation of any active helper/queue. Readiness is checked after.
        if let Some(panel) = &mut self.comparison {
            panel.cancel(true);
        }
        if let Some(panel) = &mut self.checksums {
            panel.cancel(true);
        }
        self.sftp.transport.close_ready();
        self.sftp_profiles.begin_close(self.discard_close);
        self.bookmarks.begin_close(self.discard_close);
        self.folder_views.begin_close(self.discard_close);
        self.columns.clear();
        self.dual.begin_close();
        self.cancel_outgoing_drag();
        self.clipboard_state.begin_close();
        self.native_transfer.begin_close();
        self.text_paste.invalidate();
        self.cancel_copy_dialog();
        self.recovery_catalog.close();
        self.thumbnails.begin_close();
        self.sizes.cancel_all();
        self.du_requested = None;
        if let Some(job) = &self.du_job {
            job.cancel();
        }
        if self.space_map.visible() {
            self.space_map.close();
            self.space_geometry.clear();
        }
        self.space_requested = false;
        self.copy.begin_close();
        self.cancel_delete();
        if self.preview.visible() {
            self.preview.close();
        }
        if !self.mutations_drained() {
            self.mutations.cancel();
        }
        if !self.listing_sync.stopped()
            || !self.native_transfer.is_drained()
            || !self.clipboard_state.is_drained()
            || self.move_preparing.is_some()
            || self.search_transfer.busy()
            || !self.thumbnails.close_allowed()
            || !self.space_map.is_drained()
            || !self.copy.is_closed()
            || !self.preview.exit_allowed()
            || !self.mutations_drained()
            || self.delete.busy()
        {
            return false;
        }
        if !self.sftp.transport.close_ready() {
            return false;
        }
        if !self.sftp_profiles.close_ready() {
            return false;
        }
        if !self.saved_searches.close_ready() {
            return false;
        }
        if !self.folder_views.close_ready() {
            return false;
        }
        if !self.bookmarks.close_ready() {
            return false;
        }
        if self.comparison.as_ref().is_some_and(|p| p.busy()) {
            return false;
        }
        if self.checksums.as_ref().is_some_and(|p| p.busy()) {
            return false;
        }
        if !self.session_close_ready() {
            return false;
        }
        if self.discard_close {
            return true;
        }
        self.save_layout()
            && !self.layout_dirty
            && !self.config_dirty
            && self.config_in_flight.is_none()
            && self.config_error.is_none()
            && self.saves.drain_state() == crate::save_queue::DrainState::Idle
    }
    pub fn select_entry(&mut self, i: usize, toggle: bool, extend: bool) {
        self.folder_restore = None;
        self.text_paste.invalidate();
        self.columns.deferred_menu = None;
        self.session.restore_allowed = false;
        if self.selection_refreshing()
            || !self
                .entries
                .get(i)
                .is_some_and(|e| files_core::visibility::accepts(&e.name, self.show_hidden))
        {
            return;
        }
        let order = if self.view == View::SpaceMap && !self.dual.enabled {
            self.space_geometry.navigation_indices()
        } else {
            self.order.as_slice()
        };
        if self.view == View::SpaceMap && !self.dual.enabled && !order.contains(&i) {
            return;
        }
        self.preserve_empty_selection = false;
        if self.selected != Some(i) {
            self.preview.close();
        }
        if extend {
            let from = self
                .anchor
                .filter(|a| order.contains(a))
                .or(self.selected.filter(|a| order.contains(a)))
                .and_then(|a| order.iter().position(|&j| j == a));
            let to = order.iter().position(|&j| j == i);
            if let (Some(a), Some(b)) = (from.or(to), to) {
                if !toggle {
                    self.selection.clear();
                }
                self.selection
                    .extend(order[a.min(b)..=a.max(b)].iter().copied());
            }
        } else if toggle {
            if !self.selection.insert(i) {
                self.selection.remove(&i);
            }
        } else {
            self.selection.clear();
            self.selection.insert(i);
            self.anchor = Some(i);
        }
        self.selected = Some(i);
        if self.anchor.is_none() {
            self.anchor = Some(i);
        }
    }
    pub fn spatial_position(&self, i: usize) -> Position {
        if let Some(position) = self.layout.position(&self.entries[i].name) {
            position
        } else {
            let rank = self.order.iter().position(|&j| j == i).unwrap_or(i);
            Position {
                x: (rank % 6) as f32 * 144.0,
                y: (rank / 6) as f32 * 116.0,
            }
        }
    }
    pub fn pointer_down(&mut self, x: i32, y: i32, toggle: bool, extend: bool, double: bool) {
        let action = self
            .hits
            .iter()
            .rev()
            .find(|h| h.generation == self.generation && h.rect.contains(x, y))
            .map(|h| h.action.clone());
        if let Some(Action::PaneEntry(id, rev, i)) = action {
            self.pane_select(id, rev, i, toggle, extend);
            if double && self.dual.active_id == id && self.selected == Some(i) {
                self.open_selected();
            }
            return;
        }
        if let Some(Action::ColumnEntry(id, i)) = action {
            self.column_focus(id, Some(i), toggle, extend);
            if double && self.loaded && self.selected == Some(i) {
                self.open_selected();
            }
            return;
        }
        if let Some(Action::Entry(i)) = action {
            if self.selection_refreshing()
                || self.view == View::SpaceMap
                    && !self.dual.enabled
                    && !self.space_geometry.navigation_indices().contains(&i)
                || !self
                    .entries
                    .get(i)
                    .is_some_and(|e| files_core::visibility::accepts(&e.name, self.show_hidden))
            {
                return;
            }
            if !(self.view == View::Spatial && !toggle && !extend && self.selection.contains(&i)) {
                self.select_entry(i, toggle, extend);
            }
            if double {
                self.open_selected();
                return;
            }
            if self.view == View::Spatial && self.layout_ready {
                if self.selection.len() > files_core::spatial::MAX_POSITIONS {
                    self.error = "La selección supera el límite de posiciones manuales".into();
                    return;
                }
                let positions = self
                    .order
                    .iter()
                    .copied()
                    .enumerate()
                    .filter(|(_, j)| self.selection.contains(j))
                    .map(|(rank, j)| {
                        (
                            j,
                            self.layout
                                .position(&self.entries[j].name)
                                .unwrap_or(Position {
                                    x: (rank % 6) as f32 * 144.0,
                                    y: (rank / 6) as f32 * 116.0,
                                }),
                        )
                    })
                    .collect();
                self.drag = Some(Drag {
                    generation: self.generation,
                    start: (x, y),
                    positions,
                    moved: false,
                });
            }
        } else if let Some(action) = action {
            self.action(action);
        } else if self.view == View::Spatial && y >= 94 && !self.selection_refreshing() {
            self.selection.clear();
            self.selected = None;
            self.preserve_empty_selection = true;
            self.drag = None;
        } else {
            self.settings = false;
            self.actions_menu = false;
        }
    }
    pub fn pointer_drag(&mut self, x: i32, y: i32) {
        let Some(drag) = &mut self.drag else {
            return;
        };
        if drag.generation != self.generation {
            self.drag = None;
            return;
        }
        let dx = (x - drag.start.0) as f32 / self.layout.zoom;
        let dy = (y - drag.start.1) as f32 / self.layout.zoom;
        if dx.abs() + dy.abs() < 3.0 {
            return;
        }
        drag.moved = true;
        let updates = drag
            .positions
            .iter()
            .filter_map(|(i, p)| {
                self.entries.get(*i).map(|e| {
                    (
                        e.name.as_os_str(),
                        Position {
                            x: (p.x + dx).max(0.0),
                            y: (p.y + dy).max(0.0),
                        },
                    )
                })
            })
            .collect::<Vec<_>>();
        if let Err(error) = Arc::make_mut(&mut self.layout).set_positions(&updates) {
            self.error = format!("No se movieron los iconos: {error}");
            return;
        }
        self.layout_changed();
    }
    pub fn pointer_up(&mut self) {
        if self.drag.take().is_some_and(|d| d.moved) {
            self.save_layout();
        }
    }
}
pub fn paint(p: &mut dyn Painter, m: &mut Model, area: Rect) {
    let c = m.palette.clone();
    let z = m.layout.zoom;
    p.text(
        rect(area.x, area.y, area.w - 170, 28),
        &format!(
            "{}  {}% · {} seleccionados",
            crate::fonts::icon("\u{f279}", "Espacial"),
            (z * 100.0).round(),
            m.selection.len()
        ),
        11,
        c.dark_foreground,
        true,
        false,
    );
    for (label, offset, scale) in [("−", 152, 1.0 / 1.15), ("+", 110, 1.15)] {
        let r = rect(area.x + area.w - offset, area.y + 1, 36, 26);
        p.rounded_rect(r, 6 * m.radius, c.lighter_background);
        p.text(
            rect(r.x + 12, r.y, r.w - 12, r.h),
            label,
            14,
            c.accent,
            true,
            false,
        );
        m.hits.push(Hit {
            rect: r,
            action: Action::Zoom(scale),
            generation: m.generation,
        });
    }
    let top = area.y + 32;
    let bottom = area.y + area.h;
    p.clip(rect(area.x, top, area.w, area.h - 32));
    let mut extent = 0;
    for rank in 0..m.order.len() {
        let i = m.order[rank];
        let e = &m.entries[i];
        let pos = m.layout.position(&e.name).unwrap_or(Position {
            x: (rank % 6) as f32 * 144.0,
            y: (rank / 6) as f32 * 116.0,
        });
        let x = area.x + (pos.x * z - m.pan_x).round() as i32;
        let y = top + (pos.y * z).round() as i32 - m.scroll;
        let w = (128.0 * z).round() as i32;
        let h = (100.0 * z).round() as i32;
        extent = extent.max((pos.y * z) as i32 + h + 36);
        if y + h < top || y >= bottom || x + w < area.x || x >= area.x + area.w {
            continue;
        }
        let on = m.selection.contains(&i) || m.selected == Some(i);
        if on || m.hovered == Some(i) {
            p.rounded_rect(
                rect(x, y, w, h),
                8 * m.radius,
                if on {
                    c.selection
                } else {
                    c.lighter_background
                },
            );
        }
        let name = files_core::name_label::label(&e.name);
        let mime = files_core::mime::mime(std::path::Path::new(&e.name));
        let (glyph, color) = if e.is_dir() {
            (crate::fonts::icon("\u{f07b}", "DIR"), c.blue)
        } else if mime.starts_with("image/") {
            (crate::fonts::icon("\u{f03e}", "IMG"), c.magenta)
        } else if mime.starts_with("video/") {
            (crate::fonts::icon("\u{f008}", "VID"), c.magenta)
        } else if mime.starts_with("audio/") {
            (crate::fonts::icon("\u{f001}", "AUD"), c.green)
        } else {
            (crate::fonts::icon("\u{f15b}", "FILE"), c.cyan)
        };
        p.text(
            rect(x + ((w - 48) / 2).max(0), y + 4, w.min(60), h * 2 / 3),
            glyph,
            (38.0 * z).round() as i32,
            color,
            true,
            false,
        );
        p.text(
            rect(x + 6, y + h * 2 / 3, w - 12, h / 3),
            &name,
            (11.0 * z).round().max(9.0) as i32,
            c.bright_foreground,
            true,
            false,
        );
        m.visible.push(i);
        m.hits.push(Hit {
            rect: rect(
                x.max(area.x),
                y.max(top),
                (x + w).min(area.x + area.w) - x.max(area.x),
                (y + h).min(bottom) - y.max(top),
            ),
            action: Action::Entry(i),
            generation: m.generation,
        });
    }
    m.content_height = extent.max(area.h);
    p.unclip();
}

#[cfg(test)]
mod tests {
    use super::*;
    use files_core::{
        listing::{Entry, Kind},
        spatial::SpatialLayout,
    };
    use std::{ffi::OsString, path::PathBuf};
    struct Canvas;
    impl Painter for Canvas {
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
        fn image(&mut self, _: Rect, _: &crate::painter::Image<'_>) {}
        fn clip(&mut self, _: Rect) {}
        fn unclip(&mut self) {}
    }
    fn model() -> Model {
        let mut m = Model::new(PathBuf::from("/nonexistent-files-test-fixture"));
        // Este mock de AuxIo prueba otro subsistema; el catálogo inicial ya está cargado.
        m.folder_views = Default::default();
        m.folder_views.loaded = true;
        m.folder_restore = None;
        m.entries = (0..64)
            .map(|i| Entry {
                name: OsString::from(format!("file{i}.txt")),
                kind: Kind::File,
                metadata: None,
                stat_attempted: false,
            })
            .collect();
        m.rebuild();
        m.view = View::Spatial;
        m.layout = Arc::new(SpatialLayout::default());
        m.layout_ready = true;
        m.layout_path = Some(m.path.clone());
        m.layout_requested = true;
        m
    }
    #[test]
    fn selection_drag_zoom_virtualization_share_actual_hit_targets() {
        let mut m = model();
        crate::view::paint(&mut Canvas, &mut m, 1000, 650);
        let targets = m
            .hits
            .iter()
            .filter(|h| matches!(h.action, Action::Entry(_)))
            .map(|h| (h.rect, h.action.clone()))
            .collect::<Vec<_>>();
        assert!(targets.len() < m.entries.len());
        let (a, Action::Entry(first)) = &targets[0] else {
            panic!()
        };
        let (b, Action::Entry(second)) = &targets[1] else {
            panic!()
        };
        m.pointer_down(a.x + 5, a.y + 5, false, false, false);
        m.pointer_down(b.x + 5, b.y + 5, true, false, false);
        assert_eq!(m.selection.len(), 2);
        let p = m.spatial_position(*first);
        m.pointer_drag(b.x + 35, b.y + 25);
        assert_eq!(
            m.layout.position(&m.entries[*first].name),
            Some(Position {
                x: p.x + 30.0,
                y: p.y + 20.0
            })
        );
        assert!(m.layout.position(&m.entries[*second].name).is_some());
        m.drag = None;
        m.action(Action::Zoom(1.15));
        assert!((m.layout.zoom - 1.15).abs() < 0.001);
    }
    #[test]
    fn navigation_invalidates_visible_indices_before_next_draw() {
        let mut m = model();
        m.loaded = true;
        crate::view::paint(&mut Canvas, &mut m, 1000, 650);
        let stale_generation = m.generation;
        let stale = m
            .hits
            .iter()
            .find(|h| matches!(h.action, Action::Entry(_)))
            .unwrap()
            .rect;
        assert!(!m.visible.is_empty());
        m.navigate(PathBuf::from("/nonexistent-files-test-empty"));
        assert!(m.generation != stale_generation);
        assert!(m.visible.is_empty() && m.hits.is_empty() && m.hovered.is_none());
        m.loaded = true;
        m.visible = vec![usize::MAX];
        m.visible_generation = stale_generation;
        m.request_stats();
        m.pointer_down(stale.x + 2, stale.y + 2, false, false, false);
        assert!(m.selected.is_none());
        m.visible_generation = m.generation;
        m.request_stats();
    }
    #[test]
    fn navigation_and_close_retain_failed_snapshot_until_retry_ack() {
        use crate::aux_io::{AuxIo, Request, Response};
        use std::sync::mpsc;
        let mut m = model();
        let original = m.path.clone();
        let (tx, input) = mpsc::sync_channel(1);
        let (output, rx) = mpsc::channel();
        m.aux = AuxIo { tx, rx };
        Arc::make_mut(&mut m.layout).set_zoom(1.5);
        m.layout_changed();
        assert!(m.save_layout());
        assert!(m.layout_dirty, "enqueue is not an ACK");
        let Request::Save(revision, path, snapshot) = input.try_recv().unwrap() else {
            panic!()
        };
        assert!(Arc::ptr_eq(&snapshot, &m.layout));
        m.view = View::List;
        m.navigate(PathBuf::from("/nonexistent-files-test-other"));
        output
            .send(Response::Saved(
                path.clone(),
                revision,
                Err("disk full".into()),
            ))
            .unwrap();
        m.pump();
        assert_eq!(m.saves.failure(&original).unwrap().message, "disk full");
        m.closing = true;
        assert!(!m.close_ready());
        m.retry_saves();
        let Request::Save(retry_revision, retry_path, _) = input.try_recv().unwrap() else {
            panic!()
        };
        assert_eq!(retry_path, original);
        output
            .send(Response::Saved(retry_path, retry_revision, Ok(())))
            .unwrap();
        m.pump();
        assert!(m.close_ready());
    }
    #[test]
    fn full_save_budget_refuses_navigation_without_losing_active_layout() {
        use crate::aux_io::{AuxIo, Request};
        use std::sync::mpsc;
        let mut m = model();
        let (tx, _input) = mpsc::sync_channel(1);
        let (_output, rx) = mpsc::channel();
        tx.send(Request::Load(0, "occupied".into())).unwrap();
        m.aux = AuxIo { tx, rx };
        m.saves
            .submit_shared("first".into(), 1, Arc::default())
            .unwrap();
        m.saves
            .submit_shared("second".into(), 2, Arc::default())
            .unwrap();
        m.layout_changed();
        let current = m.path.clone();
        m.navigate("third".into());
        assert_eq!(m.path, current);
        assert!(m.layout_dirty);
        assert!(!m.close_ready());
        assert!(m.error.contains("sin guardar"));
    }
    #[test]
    fn alias_load_restores_pending_canonical_snapshot_instead_of_old_disk_layout() {
        use crate::aux_io::{AuxIo, Request, Response};
        use std::sync::mpsc;
        let mut m = model();
        m.view = View::List;
        let canonical = m.path.clone();
        let (tx, input) = mpsc::sync_channel(4);
        let (output, rx) = mpsc::channel();
        m.aux = AuxIo { tx, rx };
        Arc::make_mut(&mut m.layout).set_zoom(1.5);
        m.layout_changed();
        m.save_layout();
        let Request::Save(rev, _, _) = input.try_recv().unwrap() else {
            panic!()
        };
        m.navigate(canonical.join("alias"));
        output
            .send(Response::Saved(
                canonical.clone(),
                rev,
                Err("disk full".into()),
            ))
            .unwrap();
        output
            .send(Response::Layout(
                m.generation,
                Ok((canonical.clone(), SpatialLayout::default())),
            ))
            .unwrap();
        m.pump();
        assert_eq!(m.layout_path, Some(canonical));
        assert_eq!(m.layout.zoom, 1.5);
        assert!(m.layout_dirty);
        m.retry_saves();
        let Request::Save(retry_rev, _, layout) = input.try_recv().unwrap() else {
            panic!()
        };
        assert_eq!(retry_rev, rev);
        assert_eq!(layout.zoom, 1.5);
    }

    #[test]
    fn preference_write_failure_blocks_close_and_retries_latest_value() {
        use crate::aux_io::{AuxIo, Request, Response};
        use std::sync::mpsc;
        let mut m = model();
        let (tx, input) = mpsc::sync_channel(1);
        let (output, rx) = mpsc::channel();
        m.aux = AuxIo { tx, rx };
        m.view = View::List;
        m.set_corners(files_core::theme::Corners::Square);
        let Request::Preferences(rev, _, _, _, _, _) = input.try_recv().unwrap() else {
            panic!()
        };
        m.set_corners(files_core::theme::Corners::Rounded);
        m.action(Action::WindowMode(
            files_core::theme::WindowMode::Fullscreen,
        ));
        output
            .send(Response::Config(rev, Err("read-only configuration".into())))
            .unwrap();
        m.pump();
        assert!(!m.close_ready());
        assert!(m.config_error.is_some());
        m.retry_saves();
        let Request::Preferences(latest, corners, mode, _, _, _) = input.try_recv().unwrap() else {
            panic!()
        };
        assert!(latest > rev && corners == files_core::theme::Corners::Rounded);
        assert_eq!(mode, files_core::theme::WindowMode::Fullscreen);
        output.send(Response::Config(latest, Ok(()))).unwrap();
        m.pump();
        assert!(m.close_ready());
    }
    #[test]
    fn readiness_requires_loaded_entries_and_metadata_or_spatial_layout() {
        let mut m = model();
        assert!(!m.scene_ready());
        m.loaded = true;
        m.view = View::Groups;
        assert!(
            !m.scene_ready(),
            "loaded names alone do not complete Groups"
        );
        m.view = View::List;
        assert!(m.scene_ready());
        m.view = View::Spatial;
        m.layout_ready = false;
        assert!(!m.scene_ready());
        m.layout_ready = true;
        assert!(m.scene_ready());
        m.error = "listing failed".into();
        assert!(!m.scene_ready());
    }
}
