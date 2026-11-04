use super::*;
use crate::{
    model::Action,
    painter::{Painter, Rect},
};
use std::{
    fs,
    time::{Duration, Instant},
};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let serial = ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = files_core::theme::home()
            .join(".cache/files-test")
            .join(format!(
                "columns-{}-{}-{}",
                serial,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::create_dir(&path).unwrap();
        for n in ["A", "B"] {
            fs::create_dir_all(path.join(n).join("nested")).unwrap();
            for f in ["same.txt", "alpha.txt", "last.txt", ".hidden"] {
                fs::write(path.join(n).join(f), n).unwrap();
            }
        }
        Self(path)
    }
    fn model(&self) -> Model {
        let mut m = Model::new(self.0.clone());
        m.view = View::Columns;
        m.show_hidden = false;
        wait(&mut m, |m| m.loaded);
        m
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn wait(m: &mut Model, check: impl Fn(&Model) -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        m.pump();
        if check(m) {
            return;
        }
        assert!(Instant::now() < end, "{}", m.error);
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn select(m: &mut Model, name: &str) {
    let i = m
        .entries
        .iter()
        .position(|e| e.name == name)
        .unwrap_or_else(|| {
            panic!(
                "missing {name:?}; path={:?} entries={:?} loaded={} error={:?}",
                m.path,
                m.entries.iter().map(|e| &e.name).collect::<Vec<_>>(),
                m.loaded,
                m.error
            )
        });
    m.select_entry(i, false, false);
    m.pump_columns();
}
fn child(m: &mut Model) {
    wait(m, |m| {
        m.columns
            .columns
            .get(m.columns.active + 1)
            .is_some_and(|c| c.data.loaded)
    });
}
fn foreground(m: &mut Model) {
    wait(m, |m| m.loaded && !m.selection_refreshing());
}
#[test]
fn lookahead_does_not_change_operation_origin_and_ancestor_change_prunes_stale() {
    let f = Fixture::new();
    let mut m = f.model();
    select(&mut m, "A");
    child(&mut m);
    let stale = m.columns.columns[1].id;
    assert_eq!(m.path, f.0);
    assert_eq!(m.columns.columns[1].path, f.0.join("A"));
    m.action(Action::CopyFiles);
    let form = m.copy_form.as_ref().unwrap();
    assert_eq!(form.source, f.0);
    assert_eq!(form.names.as_ref(), [std::ffi::OsString::from("A")]);
    m.cancel_copy_dialog();
    select(&mut m, "B");
    child(&mut m);
    let id = m.columns.columns[1].id;
    assert_ne!(id, stale);
    m.column_focus(stale, None, false, false);
    assert_eq!(m.path, f.0);
    m.column_focus(id, None, false, false);
    foreground(&mut m);
    assert_eq!(m.path, f.0.join("B"));
    assert!(m.columns.pending.is_none());
    assert!(m.columns.columns[m.columns.active].data.entries.is_empty());
    select(&mut m, "same.txt");
    m.action(Action::CopyFiles);
    assert_eq!(m.copy_form.as_ref().unwrap().source, f.0.join("B"));
    assert_eq!(fs::read(f.0.join("A/same.txt")).unwrap(), b"A");
}
#[test]
fn focus_before_done_rejects_partial_indices_then_suspends_promoted_owner() {
    let f = Fixture::new();
    let mut m = f.model();
    select(&mut m, "A");
    let id = m.columns.columns[1].id;
    // Deterministic: no pumping the lookahead channel before focus.
    assert!(!m.columns.columns[1].data.loaded);
    m.column_focus(id, Some(0), false, false);
    assert_eq!(m.path, f.0);
    child(&mut m);
    m.column_focus(id, None, false, false);
    assert!(m.columns.pending.is_none());
    for _ in 0..10 {
        m.pump();
    }
    foreground(&mut m);
    assert!(m.columns.columns[m.columns.active].data.entries.is_empty());
    assert_eq!(m.entries.iter().filter(|e| e.name == "same.txt").count(), 1);
}
#[test]
fn inactive_modifier_selection_and_rapid_focus_preserve_raw_snapshot() {
    let f = Fixture::new();
    let mut m = f.model();
    select(&mut m, "A");
    child(&mut m);
    let root = m.columns.columns[0].id;
    let a = m.columns.columns[1].id;
    m.column_focus(a, None, false, false);
    foreground(&mut m);
    select(&mut m, "alpha.txt");
    let first = m.selected.unwrap();
    let last = m.entries.iter().position(|e| e.name == "last.txt").unwrap();
    m.column_focus(root, None, false, false);
    foreground(&mut m);
    m.column_focus(a, Some(first), true, false);
    foreground(&mut m);
    assert!(!m.selection.contains(&m.selected.unwrap()));
    m.column_focus(root, None, false, false);
    foreground(&mut m);
    m.column_focus(a, Some(last), false, true);
    foreground(&mut m);
    let actual = m
        .selection
        .iter()
        .map(|i| m.entries[*i].name.to_string_lossy().to_string())
        .collect::<HashSet<_>>();
    assert_eq!(
        actual,
        HashSet::from(["alpha.txt".into(), "last.txt".into()])
    );
    // A->root->A->root->A without pump retains the pending selection snapshot.
    for id in [root, a, root, a] {
        m.column_focus(id, None, false, false);
    }
    foreground(&mut m);
    assert_eq!(m.selection.len(), 2);
}
#[test]
fn navigation_deeper_than_eight_retains_bounded_window_and_can_return() {
    let f = Fixture::new();
    let mut path = f.0.clone();
    for _ in 0..12 {
        path = path.join("deep");
        fs::create_dir(&path).unwrap();
    }
    let mut m = f.model();
    for _ in 0..12 {
        select(&mut m, "deep");
        child(&mut m);
        m.column_step(true);
        foreground(&mut m);
        assert!(m.columns.columns.len() <= MAX_COLUMNS);
    }
    assert_eq!(m.path, path);
    assert!(m.columns.notice.is_some());
    let old = m.path.clone();
    m.column_step(false);
    foreground(&mut m);
    assert_eq!(Some(m.path.as_path()), old.parent());
    for _ in 0..12 {
        if m.path == f.0 {
            break;
        }
        m.column_step(false);
        foreground(&mut m);
    }
    assert_eq!(m.path, f.0);
}
#[test]
fn hidden_and_budget_are_checked_before_retention_or_operation() {
    let f = Fixture::new();
    let mut m = f.model();
    select(&mut m, "A");
    child(&mut m);
    let c = &mut m.columns.columns[1];
    let hidden = c
        .data
        .entries
        .iter()
        .position(|e| e.name == ".hidden")
        .unwrap();
    c.data.selection.insert(hidden);
    c.data.selected = Some(hidden);
    m.show_hidden = true;
    m.pump_columns();
    m.show_hidden = false;
    m.pump_columns();
    assert!(m.columns.columns[1].data.selection.is_empty());
    assert_eq!(m.columns.columns[1].data.selected, None);
    let mut e = Entry {
        name: "x".into(),
        kind: files_core::listing::Kind::File,
        metadata: None,
        stat_attempted: false,
    };
    e.name.reserve(DATA_LIMIT + 1);
    let before = m.columns.columns[1].data.entries.len();
    assert!(!m.columns.accept_batch(&m, &[e], false));
    assert_eq!(m.columns.columns[1].data.entries.len(), before);
}
#[derive(Default)]
struct Recorder {
    rects: Vec<Rect>,
}
impl Painter for Recorder {
    fn rect(&mut self, r: Rect, _: files_core::theme::Color) {
        self.rects.push(r)
    }
    fn rounded_rect(&mut self, r: Rect, _: i32, _: files_core::theme::Color) {
        self.rects.push(r)
    }
    fn text(&mut self, _: Rect, _: &str, _: i32, _: files_core::theme::Color, _: bool, _: bool) {}
    fn clip(&mut self, _: Rect) {}
    fn unclip(&mut self) {}
    fn image(&mut self, _: Rect, _: &crate::painter::Image<'_>) {}
}
#[test]
fn paint_virtualizes_rows_clips_hits_and_preserves_scroll_while_loading() {
    let f = Fixture::new();
    for i in 0..150 {
        fs::write(f.0.join(format!("file{i}")), b"x").unwrap();
    }
    let mut m = f.model();
    m.selected = None;
    m.selection.clear();
    m.scroll = 140;
    let mut p = Recorder::default();
    let area = Rect::new(60, 94, 650, 490);
    crate::columns_view::paint(&mut p, &mut m, area);
    assert!(m.visible.len() < 20);
    assert!(
        m.hits
            .iter()
            .filter(|h| matches!(h.action, Action::ColumnEntry(_, _)))
            .all(|h| h.rect.x >= area.x
                && h.rect.x + h.rect.w <= area.x + area.w
                && h.rect.y >= area.y
                && h.rect.y + h.rect.h <= area.y + area.h)
    );
    m.entries.clear();
    m.order.clear();
    m.loaded = false;
    crate::columns_view::paint(&mut p, &mut m, area);
    assert_eq!(m.scroll, 140);
}

#[cfg(unix)]
#[test]
fn raw_column_names_and_filtered_operation_payload_remain_lossless() {
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
    let f = Fixture::new();
    let raw = OsStr::from_bytes(b"raw-\xff");
    fs::create_dir(f.0.join(raw)).unwrap();
    fs::write(f.0.join(raw).join("same.txt"), b"raw body").unwrap();
    let mut m = f.model();
    let i = m.entries.iter().position(|e| e.name == raw).unwrap();
    m.select_entry(i, false, false);
    m.pump_columns();
    child(&mut m);
    m.column_step(true);
    foreground(&mut m);
    assert_eq!(m.path, f.0.join(raw));
    select(&mut m, "same.txt");
    m.action(Action::Rename);
    assert_eq!(
        m.rename_dialog.as_ref().unwrap().selected.as_ref(),
        [std::ffi::OsString::from("same.txt")]
    );
}

#[test]
fn inactive_right_click_defers_one_menu_to_raw_name_and_cancels_on_new_intent() {
    let f = Fixture::new();
    let mut m = f.model();
    select(&mut m, "A");
    child(&mut m);
    let mut p = Recorder::default();
    crate::columns_view::paint(&mut p, &mut m, Rect::new(60, 94, 700, 500));
    let id = m.columns.columns[1].id;
    let index = m.columns.columns[1]
        .data
        .entries
        .iter()
        .position(|e| e.name == "same.txt")
        .unwrap();
    let r = m
        .hits
        .iter()
        .find(|hit| matches!(hit.action,Action::ColumnEntry(c,i)if c==id&&i==index))
        .unwrap()
        .rect;
    assert!(crate::context_menu::open_at(
        &mut m,
        r.x + r.w / 2,
        r.y + r.h / 2,
        1000,
        650
    ));
    assert!(m.context_menu.is_none());
    assert!(m.columns.deferred_menu.is_some());
    foreground(&mut m);
    assert_eq!(m.path, f.0.join("A"));
    assert!(m.context_menu.is_some());
    assert_eq!(m.entries[m.selected.unwrap()].name, "same.txt");
    m.context_menu = None;
    m.column_step(false);
    foreground(&mut m);
    child(&mut m);
    let id = m.columns.columns[1].id;
    crate::columns_view::paint(&mut p, &mut m, Rect::new(60, 94, 700, 500));
    let index = m.columns.columns[1]
        .data
        .entries
        .iter()
        .position(|e| e.name == "same.txt")
        .unwrap();
    let r = m
        .hits
        .iter()
        .find(|hit| matches!(hit.action,Action::ColumnEntry(c,i)if c==id&&i==index))
        .unwrap()
        .rect;
    assert!(crate::context_menu::open_at(
        &mut m,
        r.x + r.w / 2,
        r.y + r.h / 2,
        1000,
        650
    ));
    crate::input(&mut m, "last");
    foreground(&mut m);
    assert!(m.context_menu.is_none());
}

#[cfg(target_os = "linux")]
#[test]
fn watched_lookahead_refreshes_raw_names_and_revokes_old_interaction_ticket() {
    use std::os::unix::ffi::OsStringExt;
    let f = Fixture::new();
    let mut m = f.model();
    m.listing_sync.enable();
    select(&mut m, "A");
    child(&mut m);
    wait(&mut m, |m| {
        m.listing_sync.settled() && m.columns.columns.len() == 2 && m.columns.columns[1].data.loaded
    });
    let stale = m.columns.columns[1].id;
    let owner = m.columns.columns[1].watch_id;
    let raw = std::ffi::OsString::from_vec(b"external-\xff".to_vec());
    fs::write(f.0.join("A").join(&raw), b"owned").unwrap();
    wait(&mut m, |m| {
        m.listing_sync.settled()
            && m.columns
                .columns
                .get(1)
                .is_some_and(|c| c.data.loaded && c.data.entries.iter().any(|e| e.name == raw))
    });
    assert_eq!(m.columns.columns[1].watch_id, owner);
    assert_ne!(m.columns.columns[1].id, stale);
    m.column_focus(stale, Some(0), false, false);
    assert_eq!(m.path, f.0);
    let current = m.columns.columns[1].id;
    m.column_focus(current, None, false, false);
    foreground(&mut m);
    assert_eq!(m.path, f.0.join("A"));
    assert!(m.entries.iter().any(|e| e.name == raw));
    m.closing = true;
    wait(&mut m, |m| m.listing_sync.stopped());
}

#[cfg(target_os = "linux")]
#[test]
fn watched_ancestor_restores_selection_anchor_and_named_scroll_without_changing_origin() {
    let f = Fixture::new();
    for n in 0..60 {
        fs::write(f.0.join(format!("row-{n:03}")), b"owned").unwrap();
    }
    let mut m = f.model();
    m.listing_sync.enable();
    select(&mut m, "A");
    child(&mut m);
    let id = m.columns.columns[1].id;
    m.scroll = 36 * 20 + 7;
    let visible = m.entries[m.order[20]].name.clone();
    m.column_focus(id, None, false, false);
    foreground(&mut m);
    wait(&mut m, |m| {
        m.listing_sync.settled() && m.columns.columns[0].data.loaded
    });
    let active = m.path.clone();
    let watch_id = m.columns.columns[0].watch_id;
    fs::write(f.0.join("000-added"), b"owned").unwrap();
    wait(&mut m, |m| {
        m.listing_sync.settled()
            && m.columns.columns[0].data.loaded
            && m.columns.columns[0]
                .data
                .entries
                .iter()
                .any(|e| e.name == "000-added")
    });
    assert_eq!(m.path, active);
    let c = &m.columns.columns[0];
    assert_eq!(c.watch_id, watch_id);
    assert_eq!(c.data.entries[c.data.selected.unwrap()].name, "A");
    assert_eq!(c.data.entries[c.data.anchor.unwrap()].name, "A");
    assert_eq!(c.data.selection.len(), 1);
    let rank = c
        .data
        .order
        .iter()
        .position(|&i| c.data.entries[i].name == visible)
        .unwrap();
    assert_eq!(c.data.scroll, rank as i32 * 36 + 7);
    m.closing = true;
    wait(&mut m, |m| m.listing_sync.stopped());
}

#[cfg(target_os = "linux")]
#[test]
fn watched_missing_branch_revokes_cached_ancestors_and_recovers_without_navigation() {
    let f = Fixture::new();
    let mut m = f.model();
    m.listing_sync.enable();
    select(&mut m, "A");
    child(&mut m);
    let id = m.columns.columns[1].id;
    m.column_focus(id, None, false, false);
    foreground(&mut m);
    wait(&mut m, |m| {
        m.listing_sync.settled() && m.columns.columns[0].data.loaded
    });
    let stale = m.columns.columns[0].id;
    fs::rename(f.0.join("A"), f.0.join("renamed-A")).unwrap();
    wait(&mut m, |m| {
        m.columns.columns.len() == 1 && m.loaded && !m.error.is_empty()
    });
    assert_eq!(m.path, f.0.join("A"));
    m.column_focus(stale, Some(0), false, false);
    assert_eq!(m.path, f.0.join("A"));
    fs::rename(f.0.join("renamed-A"), f.0.join("A")).unwrap();
    wait(&mut m, |m| {
        m.listing_sync.settled()
            && m.loaded
            && m.error.is_empty()
            && m.entries.iter().any(|e| e.name == "same.txt")
    });
    assert_eq!(m.path, f.0.join("A"));
    m.closing = true;
    wait(&mut m, |m| m.listing_sync.stopped());
}

#[cfg(target_os = "linux")]
#[test]
fn watched_chain_stays_bounded_at_eight_and_updates_the_retained_oldest_root() {
    let f = Fixture::new();
    let mut path = f.0.clone();
    for _ in 0..10 {
        path = path.join("child");
        fs::create_dir(&path).unwrap();
    }
    let mut m = f.model();
    m.listing_sync.enable();
    for _ in 0..9 {
        select(&mut m, "child");
        wait(&mut m, |m| {
            m.loaded
                && !m.selection_refreshing()
                && m.listing_sync.settled()
                && m.columns
                    .columns
                    .get(m.columns.active + 1)
                    .is_some_and(|c| c.data.loaded)
        });
        m.column_step(true);
        foreground(&mut m);
        assert!(m.columns.columns.len() <= MAX_COLUMNS);
    }
    wait(&mut m, |m| {
        m.listing_sync.settled()
            && m.columns
                .columns
                .iter()
                .enumerate()
                .all(|(i, c)| i == m.columns.active || c.data.loaded)
    });
    assert_eq!(m.columns.columns.len(), MAX_COLUMNS);
    let oldest = m.columns.columns[0].path.clone();
    let current = m.path.clone();
    fs::write(oldest.join("external-oldest.txt"), b"owned").unwrap();
    wait(&mut m, |m| {
        m.listing_sync.settled()
            && m.columns.columns[0].data.loaded
            && m.columns.columns[0]
                .data
                .entries
                .iter()
                .any(|e| e.name == "external-oldest.txt")
    });
    assert_eq!(m.path, current);
    assert_eq!(m.columns.columns.len(), MAX_COLUMNS);
    assert!(m.columns.retained() < DATA_LIMIT);
    m.closing = true;
    wait(&mut m, |m| m.listing_sync.stopped());
}
