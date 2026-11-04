use crate::{
    model::Model,
    refresh_selection::Snapshot,
    session_codec::{self, Record, Session},
    session_state::Mode,
    tabs::{Tab, Tabs},
};
impl Model {
    pub fn begin_session(&mut self, restore: bool, save: bool) {
        if let Err(e) = self
            .session
            .begin(&self.aux.tx, self.generation, restore, save)
        {
            self.error = e.into();
        }
    }
    pub fn request_session_save(&mut self) -> bool {
        if self.session.mode == Mode::Off {
            self.begin_session(false, true);
            return false;
        }
        // Admission precedes selection snapshot, encoded buffer and all clones.
        if self.session.mode != Mode::Writable {
            self.error = self.session.notice.clone().unwrap_or_else(|| {
                if self.session.mode == Mode::Disabled {
                    "Sesión deshabilitada para esta instancia (--no-session)".into()
                } else {
                    "Sesión no habilitada para guardar".into()
                }
            });
            return false;
        }
        if self.session.pending.is_some() || self.session.in_flight {
            self.error = "Guardado de sesión en curso; espera su confirmación".into();
            return false;
        }
        if self.session.error.is_some() {
            return false;
        }
        let result = (|| {
            let owned;
            let selection = if let Some(s) = &self.refresh_selection {
                s
            } else {
                owned = Snapshot::capture_with_anchor(
                    self.entries
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| self.selection.contains(i))
                        .map(|(_, e)| e.name.as_os_str()),
                    self.selected
                        .and_then(|i| self.entries.get(i))
                        .map(|e| e.name.as_os_str()),
                    self.anchor
                        .and_then(|i| self.entries.get(i))
                        .map(|e| e.name.as_os_str()),
                    crate::tabs::SELECTION_LIMIT,
                )?;
                &owned
            };
            let active = Record {
                path: &self.path,
                history: &self.history,
                selection,
                view: self.view,
                sort: self.sort,
                descending: self.descending,
                query: &self.query,
                scroll: self.scroll,
                grid_zoom: self.grid_zoom,
                pan_x: self.pan_x,
            };
            let mut records = Vec::with_capacity(self.tabs.rows.len());
            let mut index = 0;
            for (i, t) in self.tabs.rows.iter().enumerate() {
                if t.id == self.tabs.active {
                    index = i;
                    records.push(Record { ..active });
                } else {
                    records.push(Record::from(
                        t.saved.as_ref().expect("inactive tab snapshot"),
                    ));
                }
            }
            let closed = self
                .tabs
                .closed
                .iter()
                .map(Record::from)
                .collect::<Vec<_>>();
            session_codec::encode(&records, &closed, index)
        })();
        match result.and_then(|bytes| self.session.submit(bytes)) {
            Ok(()) => {
                self.session.dispatch(&self.aux.tx);
                true
            }
            Err(e) => {
                self.session.error = Some(e.into());
                self.error = format!("Sesión sin guardar: {e}");
                false
            }
        }
    }
    pub fn session_opened(&mut self, mut opened: crate::session_store::Opened) {
        let apply = self.session.can_restore(self.generation, self.closing) && !self.editing;
        if apply && let Some(document) = opened.session.take() {
            self.apply_session(document);
        }
        self.session.opened(&opened);
        if self.session.save_after_open && !self.closing {
            self.session.save_after_open = false;
            self.request_session_save();
        }
    }
    pub fn apply_session(&mut self, session: Session) {
        let mut tabs = Tabs::new(session.tabs[0].path.clone());
        tabs.rows.clear();
        let mut active = None;
        for (index, saved) in session.tabs.into_iter().enumerate() {
            let id = tabs.fresh_id();
            let path = saved.path.clone();
            if index == session.active {
                tabs.active = id;
                active = Some(saved);
                tabs.rows.push(Tab {
                    id,
                    path,
                    saved: None,
                });
            } else {
                tabs.rows.push(Tab {
                    id,
                    path,
                    saved: Some(saved),
                });
            }
        }
        tabs.closed = session.closed;
        self.tabs = tabs;
        self.restore_tab(active.expect("validated active tab"));
    }
    pub fn session_close_ready(&mut self) -> bool {
        if self.session.mode == Mode::Opening {
            return false;
        }
        if self.discard_close {
            return !self.session.in_flight;
        }
        if self.session.mode != Mode::Writable {
            return true;
        }
        if self.session.error.is_some() {
            return false;
        }
        if !self.session.closing_saved {
            if self.session.pending.is_some() || self.session.in_flight {
                return false;
            }
            if !self.request_session_save() {
                return false;
            }
            self.session.closing_saved = true;
        }
        self.session.dispatch(&self.aux.tx);
        self.session.pending.is_none() && !self.session.in_flight && self.session.error.is_none()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model::View, session_store::Opened};
    use files_core::{
        listing::{Entry, Kind, Sort},
        navigation_history::History,
    };
    use std::path::Path;
    fn document() -> Session {
        Session {
            tabs: vec![crate::tabs::Saved {
                path: "/session-delayed-other".into(),
                history: History::default(),
                selection: Snapshot::capture(std::iter::empty(), None).unwrap(),
                view: View::List,
                sort: Sort::Name,
                descending: false,
                query: String::new(),
                scroll: 0,
                grid_zoom: 1.,
                pan_x: 0.,
                charge: 80000,
            }],
            closed: Vec::new(),
            active: 0,
        }
    }
    fn opening(m: &mut Model) {
        m.session.mode = Mode::Opening;
        m.session.restore_allowed = true;
        m.session.opening_generation = m.generation;
    }
    #[test]
    fn actual_selection_filter_navigation_or_close_rejects_late_open() {
        for action in 0..5 {
            let mut m = Model::new("/session-current-not-created".into());
            m.entries = vec![Entry {
                name: "a".into(),
                kind: Kind::File,
                metadata: None,
                stat_attempted: false,
            }];
            m.loaded = true;
            m.rebuild();
            opening(&mut m);
            let initial_generation = m.generation;
            match action {
                0 => m.select_entry(0, false, false),
                1 => crate::input(&mut m, "a"),
                2 => m.move_selection(1),
                3 => {
                    m.closing = true;
                }
                _ => m.navigate("/session-navigated".into()),
            };
            let path = m.path.clone();
            let actual_gen = m.generation;
            m.session_opened(Opened {
                session: Some(document()),
                writable: true,
                notice: None,
            });
            assert_eq!(m.path, path);
            assert_eq!(m.generation, actual_gen);
            assert!(!m.preview.visible());
            if action != 4 {
                assert_eq!(m.generation, initial_generation);
            }
        }
    }
    #[test]
    fn capture_error_blocks_repeated_close_until_explicit_retry() {
        let mut m = Model::new("/session-budget-not-created".into());
        m.session.mode = Mode::Writable;
        m.entries = (0..5000)
            .map(|i| Entry {
                name: format!("{i}-{}", "x".repeat(100)).into(),
                kind: Kind::File,
                metadata: None,
                stat_attempted: false,
            })
            .collect();
        m.loaded = true;
        m.rebuild();
        m.action(crate::model::Action::Selection(
            crate::selection_tools::Command::All,
        ));
        assert!(!m.session_close_ready());
        let error = m.session.error.clone();
        assert!(error.is_some());
        m.selection.clear();
        m.selected = None;
        for _ in 0..4 {
            assert!(!m.session_close_ready());
            assert!(m.session.pending.is_none());
            assert_eq!(m.session.error, error);
        }
        m.session.retry();
        assert!(!m.session_close_ready());
        assert!(m.session.pending.is_some());
    }
    #[test]
    fn disabled_or_pending_rejects_before_snapshot_work() {
        let mut m = Model::new("/session-disabled-not-created".into());
        m.session.mode = Mode::Disabled;
        assert!(!m.request_session_save());
        assert!(m.session.pending.is_none());
        m.session.mode = Mode::Writable;
        m.session.submit(vec![1]).unwrap();
        assert!(!m.request_session_save());
        assert_eq!(m.session.pending.as_ref().unwrap().as_slice(), [1]);
        assert!(m.session.error.is_none());
    }
    #[test]
    fn restoration_applies_only_a_valid_current_startup() {
        let mut m = Model::new("/session-start-not-created".into());
        opening(&mut m);
        m.session_opened(Opened {
            session: Some(document()),
            writable: true,
            notice: None,
        });
        assert_eq!(m.path, Path::new("/session-delayed-other"));
        assert!(m.refresh_selection.is_some());
        assert!(m.query.is_empty());
        assert!(!m.preview.visible());
        assert!(!m.selection.contains(&0));
        assert_eq!(m.view as u8, View::List as u8);
    }
}
