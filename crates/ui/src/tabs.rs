//! Bounded metadata only. Exactly one Model/listing/preview pipeline remains active.
use crate::{model::View, refresh_selection::Snapshot};
use files_core::{listing::Sort, navigation_history::History};
use std::path::PathBuf;
pub const HEIGHT: i32 = 32;
pub const MAX_TABS: usize = 8;
pub const SNAPSHOT_LIMIT: usize = 1024 * 1024;
pub const SELECTION_LIMIT: usize = 256 * 1024;
// Additional retained state, not an app/PSS cap. Reserve1MiB for active snapshot
// construction/history/query and128KiB for row paths/container slack. Stored
// inactive+closed snapshots may use at most3MiB including charged capacities.
pub const STORED_LIMIT: usize = 3 * 1024 * 1024;
pub struct Saved {
    pub path: PathBuf,
    pub history: History,
    pub selection: Snapshot,
    pub view: View,
    pub sort: Sort,
    pub descending: bool,
    pub query: String,
    pub scroll: i32,
    pub grid_zoom: f32,
    pub pan_x: f32,
    pub charge: usize,
}
pub struct Tab {
    pub id: u64,
    pub path: PathBuf,
    pub saved: Option<Saved>,
}
pub struct Tabs {
    pub rows: Vec<Tab>,
    pub active: u64,
    pub closed: Vec<Saved>,
    next: u64,
    pub first: usize,
}
#[derive(Clone, Copy, Debug)]
pub enum Command {
    New,
    Select(u64),
    Close(u64),
    Cycle(i32),
    Reopen,
}
impl Tabs {
    pub fn new(path: PathBuf) -> Self {
        Self {
            rows: vec![Tab {
                id: 1,
                path,
                saved: None,
            }],
            active: 1,
            closed: Vec::new(),
            next: 1,
            first: 0,
        }
    }
    pub fn stored_bytes(&self) -> usize {
        self.rows
            .iter()
            .filter_map(|t| t.saved.as_ref())
            .chain(&self.closed)
            .map(|s| s.charge)
            .sum()
    }
    #[cfg(test)]
    pub fn admit(&self, outgoing: usize, released: usize) -> Result<(), &'static str> {
        self.admit_with_reserved(outgoing, released, 0)
    }
    pub fn admit_with_reserved(
        &self,
        outgoing: usize,
        released: usize,
        reserved: usize,
    ) -> Result<(), &'static str> {
        if outgoing > SNAPSHOT_LIMIT
            || self
                .stored_bytes()
                .checked_sub(released)
                .and_then(|n| n.checked_add(outgoing))
                .and_then(|n| n.checked_add(reserved))
                .is_none_or(|n| n > STORED_LIMIT)
        {
            return Err(
                "Pestañas: presupuesto de estado agotado; reduce selección o cierra pestañas",
            );
        }
        Ok(())
    }
    pub fn index(&self, id: u64) -> Option<usize> {
        self.rows.iter().position(|t| t.id == id)
    }
    pub fn fresh_id(&mut self) -> u64 {
        self.next = self.next.checked_add(1).expect("tab ID exhausted");
        self.next
    }
    pub fn update_path(&mut self, path: &std::path::Path) {
        if let Some(i) = self.index(self.active) {
            self.rows[i].path = path.to_path_buf();
        }
    }
    pub fn push_closed(&mut self, saved: Saved) {
        if self.closed.len() == MAX_TABS {
            self.closed.remove(0);
        }
        self.closed.push(saved);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn saved(charge: usize) -> Saved {
        Saved {
            path: "/a".into(),
            history: History::default(),
            selection: Snapshot::capture(std::iter::empty(), None).unwrap(),
            view: View::List,
            sort: Sort::Name,
            descending: false,
            query: String::new(),
            scroll: 0,
            grid_zoom: 1.,
            pan_x: 0.,
            charge,
        }
    }
    #[test]
    fn aggregate_budget_counts_live_and_closed_with_exact_release() {
        let mut tabs = Tabs::new("/a".into());
        tabs.closed.push(saved(SNAPSHOT_LIMIT));
        tabs.rows.push(Tab {
            id: 2,
            path: "/b".into(),
            saved: Some(saved(SNAPSHOT_LIMIT)),
        });
        assert!(tabs.admit(SNAPSHOT_LIMIT, 0).is_ok());
        assert!(tabs.admit(SNAPSHOT_LIMIT + 1, 0).is_err());
        tabs.closed.push(saved(SNAPSHOT_LIMIT));
        assert!(tabs.admit(1, 0).is_err());
        assert!(tabs.admit(SNAPSHOT_LIMIT, SNAPSHOT_LIMIT).is_ok());
        assert!(tabs.admit(1, STORED_LIMIT + 1).is_err());
    }
    #[test]
    fn closed_metadata_and_ids_are_bounded_and_never_reused() {
        let mut tabs = Tabs::new("/a".into());
        for _ in 0..20 {
            tabs.push_closed(saved(1));
        }
        assert_eq!(tabs.closed.len(), 8);
        let a = tabs.fresh_id();
        let b = tabs.fresh_id();
        assert!(b > a);
        assert!(tabs.index(a).is_none());
    }
}
