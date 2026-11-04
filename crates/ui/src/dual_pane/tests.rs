use super::*;
use crate::{
    model::Action,
    painter::{Painter, Rect},
    tabs::Command,
};
use std::{
    ffi::OsStr,
    fs,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static ID: AtomicU64 = AtomicU64::new(0);
        let root = files_core::theme::home()
            .join(".cache/files-test")
            .join(format!(
                "dual-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(root.parent().unwrap()).unwrap();
        fs::create_dir(&root).unwrap();
        for name in ["A", "B"] {
            fs::create_dir(root.join(name)).unwrap();
            for n in ["same.txt", "other.txt", ".hidden"] {
                fs::write(root.join(name).join(n), format!("{name}/{n}")).unwrap();
            }
            fs::create_dir(root.join(name).join("folder")).unwrap();
        }
        Self(root)
    }
    fn model(&self) -> Model {
        let mut m = Model::new(self.0.join("A"));
        m.view = View::List;
        m.show_hidden = false;
        wait(&mut m, |m| m.loaded);
        m
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn wait(m: &mut Model, f: impl Fn(&Model) -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        m.pump();
        if f(m) {
            return;
        }
        assert!(Instant::now() < end, "{}", m.error);
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn ready(m: &mut Model) {
    wait(m, |m| {
        m.loaded
            && !m.selection_refreshing()
            && m.dual
                .other
                .as_ref()
                .is_none_or(|b| !m.dual.enabled || b.loaded && b.refresh_selection.is_none())
    });
}
fn select(m: &mut Model, name: &OsStr) {
    let i = m
        .entries
        .iter()
        .position(|e| e.name == name)
        .unwrap_or_else(|| panic!("missing {name:?} {:?} {}", m.path, m.error));
    m.select_entry(i, false, false);
}
fn setup(m: &mut Model, path: PathBuf) {
    m.action(Action::DualPane);
    m.switch_dual();
    m.navigate(path);
    ready(m);
    m.switch_dual();
    ready(m);
}
#[test]
fn independent_paths_history_filter_selection_and_namespaced_tab_ids() {
    let f = Fixture::new();
    let mut m = f.model();
    select(&mut m, OsStr::new("same.txt"));
    crate::input(&mut m, "same");
    let a = m.pane_owner();
    setup(&mut m, f.0.join("B"));
    assert_eq!(m.query, "same");
    assert_eq!(m.path, f.0.join("A"));
    assert_eq!(m.pane_owner(), a);
    m.switch_dual();
    assert_eq!(m.path, f.0.join("B"));
    assert!(m.query.is_empty());
    assert_ne!(m.pane_owner().0, a.0);
    assert_eq!(m.pane_owner().1, a.1);
    select(&mut m, OsStr::new("other.txt"));
    m.navigate(f.0.join("B/folder"));
    ready(&mut m);
    m.travel_history(false);
    ready(&mut m);
    assert_eq!(m.path, f.0.join("B"));
    m.switch_dual();
    assert_eq!(m.query, "same");
    assert_eq!(m.entries[m.selected.unwrap()].name, "same.txt");
    m.action(Action::DualPane);
    assert!(!m.dual.enabled);
    let other = m.dual.other.as_ref().unwrap();
    assert!(other.entries.is_empty() && other.order.is_empty() && other.stats_requested.is_empty());
    assert_eq!(other.path, f.0.join("B"));
    m.action(Action::View(View::Columns));
    m.pump_columns();
    assert!(m.dual.other.as_ref().unwrap().worker.is_none());
    m.action(Action::DualPane);
    assert!(m.columns.worker.is_none());
    assert!(m.dual.enabled);
    ready(&mut m);
}
#[cfg(unix)]
#[test]
fn raw_opposite_destination_copy_move_and_closed_panel_do_not_redirect_queue() {
    use std::os::unix::ffi::OsStrExt;
    let f = Fixture::new();
    let dest = f.0.join(OsStr::from_bytes(b"B-\xfe"));
    fs::create_dir(&dest).unwrap();
    fs::write(dest.join("same.txt"), b"B distinct").unwrap();
    let copy = OsStr::from_bytes(b"copy-\xff");
    let moving = OsStr::from_bytes(b"move-\xff");
    fs::write(f.0.join("A").join(copy), b"copy raw").unwrap();
    fs::write(f.0.join("A").join(moving), b"move raw").unwrap();
    let mut m = f.model();
    setup(&mut m, dest.clone());
    let source_owner = m.pane_owner();
    select(&mut m, copy);
    m.action(Action::CopyFiles);
    let form = m.copy_form.as_ref().unwrap();
    assert_eq!(form.destination_path(), dest);
    assert_eq!(form.names.as_ref(), [copy.to_os_string()]);
    m.start_copy();
    let id = m.copy_id;
    assert_eq!(m.copy_owners.get(&id), Some(&source_owner));
    m.switch_dual();
    crate::input(&mut m, "same");
    select(&mut m, OsStr::new("same.txt"));
    wait(&mut m, |m| {
        m.copy_last.as_ref().is_some_and(|o| o.id == id)
            && !m.copy_busy()
            && !m.selection_refreshing()
    });
    assert!(m.copy_last.as_ref().unwrap().result.is_ok());
    assert_eq!(fs::read(dest.join(copy)).unwrap(), b"copy raw");
    assert_eq!(m.query, "same");
    assert_eq!(m.path, dest);
    assert_eq!(m.entries[m.selected.unwrap()].name, "same.txt");
    m.switch_dual();
    select(&mut m, moving);
    m.action(Action::MoveFiles);
    assert_eq!(m.copy_form.as_ref().unwrap().destination_path(), dest);
    m.copy_form.as_mut().unwrap().start_paused = true;
    m.start_copy();
    wait(&mut m, |m| {
        m.copy_form.as_ref().is_some_and(|f| f.prepared.is_some())
    });
    m.start_copy();
    let move_id = m.copy_id;
    wait(&mut m, |m| {
        m.copy.phase() == crate::copy_state::Phase::Paused
    });
    m.switch_dual();
    let closed = m.dual.active_id;
    m.tab_command(Command::Close(m.tabs.active));
    assert!(!m.dual.enabled && !m.closing);
    assert_ne!(m.dual.other.as_ref().unwrap().id, closed);
    assert!(m.copy_busy());
    m.copy.resume(move_id);
    wait(&mut m, |m| {
        m.copy_last.as_ref().is_some_and(|o| o.id == move_id) && !m.copy_busy()
    });
    assert!(m.copy_last.as_ref().unwrap().result.is_ok());
    assert!(!f.0.join("A").join(moving).exists());
    assert_eq!(fs::read(dest.join(moving)).unwrap(), b"move raw");
    assert_eq!(fs::read(f.0.join("A").join(copy)).unwrap(), b"copy raw");
}
#[derive(Default)]
struct Recorder;
impl Painter for Recorder {
    fn rect(&mut self, _: Rect, _: files_core::theme::Color) {}
    fn rounded_rect(&mut self, _: Rect, _: i32, _: files_core::theme::Color) {}
    fn text(&mut self, _: Rect, _: &str, _: i32, _: files_core::theme::Color, _: bool, _: bool) {}
    fn clip(&mut self, _: Rect) {}
    fn unclip(&mut self) {}
    fn image(&mut self, _: Rect, _: &crate::painter::Image<'_>) {}
}
#[test]
fn context_other_batch_background_keyboard_and_stale_revision_are_safe() {
    let f = Fixture::new();
    let mut m = f.model();
    setup(&mut m, f.0.join("B"));
    select(&mut m, OsStr::new("same.txt"));
    let other = m
        .entries
        .iter()
        .position(|e| e.name == "other.txt")
        .unwrap();
    m.select_entry(other, true, false);
    let a = m.dual.active_id;
    m.switch_dual();
    select(&mut m, OsStr::new("same.txt"));
    m.dual_width = 916;
    let mut p = Recorder;
    crate::dual_view::paint(&mut p, &mut m, Rect::new(68, 94, 916, 516));
    let selected_index = m
        .dual
        .other
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .position(|e| e.name == "same.txt")
        .unwrap();
    assert!(
        m.dual
            .other
            .as_ref()
            .unwrap()
            .selection
            .contains(&selected_index)
    );
    let hit = m
        .hits
        .iter()
        .find(|h| matches!(h.action,Action::PaneEntry(id,_,i)if id==a && i==selected_index))
        .unwrap()
        .rect;
    assert!(crate::context_menu::open_at(
        &mut m,
        hit.x + 10,
        hit.y + 5,
        1000,
        650
    ));
    assert_eq!(m.dual.active_id, a);
    assert_eq!(m.selection.len(), 2);
    assert!(m.context_menu.is_some());
    crate::context_menu::dismiss(&mut m);
    m.switch_dual();
    crate::dual_view::paint(&mut p, &mut m, Rect::new(68, 94, 916, 516));
    let folder = m
        .dual
        .other
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .position(|e| e.name == "folder")
        .unwrap();
    let hit = m
        .hits
        .iter()
        .find(|h| matches!(h.action,Action::PaneEntry(id,_,i)if id==a && i==folder))
        .unwrap()
        .rect;
    assert!(crate::context_menu::open_at(
        &mut m,
        hit.x + 10,
        hit.y + 5,
        1000,
        650
    ));
    assert_eq!(m.dual.active_id, a);
    assert_eq!(m.selection.len(), 1);
    assert_eq!(m.entries[m.selected.unwrap()].name, "folder");
    crate::context_menu::dismiss(&mut m);
    m.switch_dual();
    crate::dual_view::paint(&mut p, &mut m, Rect::new(68, 94, 916, 516));
    let other_id = m.dual.other.as_ref().unwrap().id;
    let rev = m.dual.other.as_ref().unwrap().revision;
    assert!(crate::context_menu::open_at(&mut m, 100, 520, 1000, 650));
    assert_eq!(m.dual.active_id, other_id);
    assert!(m.selection.is_empty());
    assert!(m.dual.other.as_ref().unwrap().selection.len() == 1);
    crate::context_menu::dismiss(&mut m);
    select(&mut m, OsStr::new("same.txt"));
    m.refresh_current();
    ready(&mut m);
    let selected = m.selected;
    assert!(!m.pane_background(other_id, rev));
    assert_eq!(m.selected, selected);
    m.hits.clear();
    m.hits.push(crate::model::Hit {
        generation: m.generation,
        rect: Rect::new(100, 200, 100, 40),
        action: Action::PaneBackground(other_id, rev),
    });
    assert!(!crate::context_menu::open_at(&mut m, 110, 210, 1000, 650));
    assert_eq!(m.selected, selected);
    m.selected = None;
    m.selection.clear();
    let active = m.dual.active_id;
    assert!(crate::context_menu::open_keyboard(&mut m, 1000, 650));
    assert_eq!(m.dual.active_id, active);
}
#[test]
fn both_views_of_same_path_reload_after_rename_and_undo_without_focus_change() {
    let f = Fixture::new();
    let mut m = f.model();
    m.action(Action::DualPane);
    ready(&mut m);
    let owner = m.pane_owner();
    select(&mut m, OsStr::new("same.txt"));
    m.action(Action::Rename);
    let d = m.rename_dialog.as_mut().unwrap();
    d.fields[0] = "renamed.txt".into();
    d.change();
    // Worker plan is submitted by pump; Apply only after reviewed preview.
    wait(&mut m, |m| {
        m.rename_dialog
            .as_ref()
            .is_some_and(|d| d.prepared.is_some())
    });
    m.action(Action::RenameApply);
    wait(&mut m, |m| {
        !m.mutations.busy
            && !m.mutation_applied
            && m.entries.iter().any(|e| e.name == "renamed.txt")
            && m.dual
                .other
                .as_ref()
                .is_some_and(|b| b.loaded && b.entries.iter().any(|e| e.name == "renamed.txt"))
    });
    assert_eq!(m.pane_owner(), owner);
    m.action(Action::Undo(false));
    wait(&mut m, |m| {
        !m.mutations.busy
            && !m.mutation_applied
            && m.entries.iter().any(|e| e.name == "same.txt")
            && m.dual
                .other
                .as_ref()
                .is_some_and(|b| b.loaded && b.entries.iter().any(|e| e.name == "same.txt"))
    });
    assert_eq!(m.pane_owner(), owner);
    assert_eq!(fs::read(f.0.join("A/same.txt")).unwrap(), b"A/same.txt");
}
#[test]
fn shared_tab_budget_includes_passive_and_hidden_group_state() {
    let f = Fixture::new();
    let mut m = f.model();
    m.action(Action::DualPane);
    ready(&mut m);
    let base = m.dual.other_stored();
    assert!(base >= 64 * 1024);
    assert!(
        m.tabs
            .admit_with_reserved(crate::tabs::STORED_LIMIT, 0, base)
            .is_err()
    );
    assert!(
        m.tabs
            .admit_with_reserved(crate::tabs::SNAPSHOT_LIMIT, 0, base)
            .is_ok()
    );
    m.action(Action::DualPane);
    assert!(!m.dual.enabled);
    assert!(m.dual.other_stored() >= base);
}

#[test]
fn inactive_size_order_does_not_treat_directory_metadata_as_a_total() {
    use files_core::listing::{Kind, Metadata};
    let mut browser = Browser::new(99, "/owned-not-opened".into(), None);
    let entry = |name: &str, kind, size: Option<u64>| Entry {
        name: name.into(),
        kind,
        metadata: size.map(|size| Metadata {
            size,
            modified: std::time::SystemTime::UNIX_EPOCH,
        }),
        stat_attempted: true,
    };
    browser.entries = vec![
        entry("z-dir", Kind::Directory, Some(1)),
        entry("a-dir", Kind::Directory, Some(9999)),
        entry("zero", Kind::File, Some(0)),
        entry("seven", Kind::File, Some(7)),
        entry("unknown", Kind::File, None),
    ];
    browser.selected = Some(2);
    browser.selection.insert(2);
    browser.sort = Sort::Size;
    for (descending, expected) in [(false, vec![1, 0, 2, 3, 4]), (true, vec![1, 0, 3, 2, 4])] {
        browser.descending = descending;
        browser.rebuild(false, &Default::default());
        assert_eq!(browser.order, expected);
        assert_eq!(browser.selected, Some(2));
        assert!(browser.selection.contains(&2));
    }
    let mut sizes = crate::size_queue::State::default();
    sizes
        .enqueue(
            &browser.path,
            [OsStr::new("a-dir"), OsStr::new("z-dir")].into_iter(),
        )
        .unwrap();
    sizes.dispatch();
    sizes.finish(Ok((1000, 1)));
    sizes.dispatch();
    sizes.finish(Ok((1, 1)));
    browser.descending = false;
    browser.rebuild(false, &sizes);
    assert_eq!(browser.order, vec![0, 1, 2, 3, 4]);
    browser.descending = true;
    browser.rebuild(false, &sizes);
    assert_eq!(browser.order, vec![1, 0, 3, 2, 4]);
}

#[test]
fn late_direct_summary_is_retained_by_the_inactive_browser() {
    let fixture = Fixture::new();
    let mut m = fixture.model();
    m.action(Action::DualPane);
    wait(&mut m, |m| m.dual.other.as_ref().is_some_and(|b| b.loaded));
    let b = m.dual.other.as_mut().unwrap();
    let index = b.entries.iter().position(|e| e.name == "folder").unwrap();
    b.requested.insert(index);
    b.worker
        .as_ref()
        .unwrap()
        .tx
        .try_send(crate::worker::Job::Summary(index, "folder".into()))
        .unwrap();
    wait(&mut m, |m| {
        m.dual
            .other
            .as_ref()
            .unwrap()
            .summaries
            .contains_key(&index)
    });
    let summary = m.dual.other.as_ref().unwrap().summaries[&index];
    assert_eq!(summary.entries, 0);
    assert_eq!(summary.status, crate::folder_summary::Status::Complete);
    let other = m.dual.other.as_ref().unwrap().id;
    m.action(Action::PaneFocus(other));
    assert_eq!(m.dual.active_id, other);
    assert_eq!(m.summaries[&index], summary);
    assert!(m.requested.contains(&index));
}

#[test]
fn forgetting_size_reorders_both_views_without_changing_their_selection() {
    let f = Fixture::new();
    let mut m = f.model();
    let entry = |name: &str| Entry {
        name: name.into(),
        kind: files_core::listing::Kind::Directory,
        metadata: None,
        stat_attempted: false,
    };
    m.entries = vec![entry("large"), entry("small")];
    m.sort = Sort::Size;
    m.descending = true;
    m.selected = Some(0);
    m.selection.insert(0);
    let mut other = Browser::new(99, m.path.clone(), None);
    other.entries = m.entries.clone();
    other.sort = Sort::Size;
    other.descending = true;
    other.selected = Some(0);
    other.selection.insert(0);
    m.dual.other = Some(other);
    m.sizes
        .enqueue(
            &m.path,
            [OsStr::new("large"), OsStr::new("small")].into_iter(),
        )
        .unwrap();
    m.sizes.dispatch();
    m.sizes.finish(Ok((20, 1)));
    m.sizes.dispatch();
    m.sizes.finish(Ok((8, 1)));
    m.rebuild_size_orders();
    assert_eq!(m.order, [0, 1]);
    assert_eq!(m.dual.other.as_ref().unwrap().order, [0, 1]);
    let id = m.sizes.rows(&m.path)[0].id;
    assert!(m.sizes.forget_id(m.sizes.epoch(), id));
    m.rebuild_size_orders();
    assert_eq!(m.order, [1, 0]);
    let other = m.dual.other.as_ref().unwrap();
    assert_eq!(other.order, [1, 0]);
    assert_eq!(m.selected, Some(0));
    assert_eq!(other.selected, Some(0));
    assert!(other.selection.contains(&0));
}

#[test]
fn size_completion_reorders_quiet_inactive_browser_even_when_active_sort_is_name() {
    let f = Fixture::new();
    let mut m = f.model();
    let entry = |name: &str| Entry {
        name: name.into(),
        kind: files_core::listing::Kind::Directory,
        metadata: None,
        stat_attempted: true,
    };
    m.entries = vec![entry("large"), entry("small")];
    m.sort = Sort::Name;
    m.selected = Some(0);
    m.selection.insert(0);
    let mut other = Browser::new(99, m.path.clone(), None);
    other.entries = m.entries.clone();
    other.sort = Sort::Size;
    other.descending = true;
    other.selected = Some(1);
    other.selection.insert(1);
    m.dual.other = Some(other);
    m.sizes
        .enqueue(
            &m.path,
            [OsStr::new("small"), OsStr::new("large")].into_iter(),
        )
        .unwrap();
    m.sizes.dispatch();
    m.sizes.finish(Ok((8, 1)));
    m.rebuild_size_orders();
    assert_eq!(m.dual.other.as_ref().unwrap().order, [1, 0]);
    m.sizes.dispatch();
    m.du_job = Some(crate::du_job::Job::completed(Ok(
        files_core::folder_usage::Measurement::apparent(20, 1),
    )));
    m.pump();
    assert_eq!(m.sizes.measured(&m.path, OsStr::new("large")), Some(20));
    let other = m.dual.other.as_ref().unwrap();
    assert_eq!(other.order, [0, 1]);
    assert_eq!(other.selected, Some(1));
    assert!(other.selection.contains(&1));
}

#[test]
fn size_recalculation_invalidates_inactive_order_via_browser_and_manager() {
    for via_manager in [false, true] {
        let f = Fixture::new();
        let mut m = f.model();
        let entry = |name: &str| Entry {
            name: name.into(),
            kind: files_core::listing::Kind::Directory,
            metadata: None,
            stat_attempted: true,
        };
        m.entries = vec![entry("large"), entry("small")];
        m.sort = Sort::Name;
        m.selected = Some(0);
        m.selection.insert(0);
        let mut other = Browser::new(99, m.path.clone(), None);
        other.entries = m.entries.clone();
        other.sort = Sort::Size;
        other.descending = true;
        other.selected = Some(1);
        other.selection.insert(1);
        m.dual.other = Some(other);
        m.sizes
            .enqueue(
                &m.path,
                [OsStr::new("large"), OsStr::new("small")].into_iter(),
            )
            .unwrap();
        m.sizes.dispatch();
        m.sizes.finish(Ok((20, 1)));
        m.sizes.dispatch();
        m.sizes.finish(Ok((8, 1)));
        m.rebuild_size_orders();
        assert_eq!(m.dual.other.as_ref().unwrap().order, [0, 1]);
        if via_manager {
            m.action(Action::Sizes);
            let stamp = m.size_panel.as_ref().unwrap().stamp;
            let id = m.sizes.rows(&m.path)[0].id;
            m.action(Action::SizePanel(
                stamp,
                crate::size_panel::Command::Recalculate(id),
            ));
        } else {
            m.request_folder_sizes(false);
        }
        assert_eq!(m.sizes.measured(&m.path, OsStr::new("large")), None);
        assert_eq!(m.dual.other.as_ref().unwrap().order, [1, 0]);
        assert_eq!(m.dual.other.as_ref().unwrap().selected, Some(1));
    }
}

#[test]
fn sorting_inactive_header_preserves_owner_selection_sizes_and_rejects_stale_revision() {
    let f = Fixture::new();
    let mut m = f.model();
    m.action(Action::DualPane);
    ready(&mut m);
    let b = m.dual.other.as_ref().unwrap();
    let (id, rev) = (b.id, b.revision);
    let generation = m.generation;
    let owner = m.pane_owner();
    let selection = m.selection.clone();
    m.sizes
        .enqueue(&m.path, [OsStr::new("folder")].into_iter())
        .unwrap();
    m.sizes.dispatch();
    m.sizes.finish(Ok((12, 1)));
    let epoch = m.sizes.epoch();
    m.action(Action::PaneSort(id, rev, Sort::Size));
    let b = m.dual.other.as_ref().unwrap();
    assert_eq!(b.sort, Sort::Size);
    assert!(b.descending);
    assert_eq!(m.sort, Sort::Name);
    assert_eq!(m.generation, generation);
    assert_eq!(m.pane_owner(), owner);
    assert_eq!(m.selection, selection);
    assert_eq!(m.sizes.epoch(), epoch);
    assert_eq!(m.sizes.measured(&m.path, OsStr::new("folder")), Some(12));
    m.action(Action::PaneSort(id, rev, Sort::Size));
    assert!(!m.dual.other.as_ref().unwrap().descending);
    m.action(Action::PaneSort(id, rev + 1, Sort::Modified));
    assert_eq!(m.dual.other.as_ref().unwrap().sort, Sort::Size);
    m.action(Action::PaneSort(id, rev, Sort::Modified));
    assert!(m.dual.other.as_ref().unwrap().descending);
    m.action(Action::PaneSort(
        m.dual.active_id,
        m.pane_revision,
        Sort::Name,
    ));
    assert!(m.descending);
    assert_eq!(m.dual.other.as_ref().unwrap().sort, Sort::Modified);
    m.dual.other.as_mut().unwrap().refresh_selection =
        Some(crate::refresh_selection::Snapshot::capture(std::iter::empty(), None).unwrap());
    m.action(Action::PaneSort(id, rev, Sort::Name));
    assert_eq!(m.dual.other.as_ref().unwrap().sort, Sort::Modified);
}

#[test]
fn sort_header_targets_are_disjoint_inside_narrow_and_wide_panels() {
    let f = Fixture::new();
    let mut m = f.model();
    m.action(Action::DualPane);
    ready(&mut m);
    for width in [356, 716, 916] {
        let mut p = Recorder;
        m.hits.clear();
        let area = Rect::new(68, 94, width, 516);
        crate::dual_view::paint(&mut p, &mut m, area);
        let hits = m
            .hits
            .iter()
            .filter(|h| matches!(h.action, Action::PaneSort(..)))
            .collect::<Vec<_>>();
        assert_eq!(hits.len(), 6);
        for (i, a) in hits.iter().enumerate() {
            assert!(a.rect.x >= area.x && a.rect.x + a.rect.w <= area.x + area.w);
            assert!(a.action.accessible_name().is_some());
            for b in hits.iter().skip(i + 1) {
                assert!(a.rect.x + a.rect.w <= b.rect.x || b.rect.x + b.rect.w <= a.rect.x);
            }
        }
    }
}

#[test]
fn clear_inactive_filter_preserves_active_owner_du_and_explicit_empty_marks() {
    let f = Fixture::new();
    let mut m = f.model();
    m.action(Action::DualPane);
    ready(&mut m);
    m.query = "other".into();
    m.rebuild();
    let b = m.dual.other.as_mut().unwrap();
    b.query = "same".into();
    b.rebuild(false, &m.sizes);
    b.selected = None;
    b.selection.clear();
    b.preserve_empty_selection = true;
    let (id, rev) = (b.id, b.revision);
    m.sizes
        .enqueue(&m.path, [OsStr::new("folder")].into_iter())
        .unwrap();
    m.sizes.dispatch();
    m.sizes.finish(Ok((9, 1)));
    let epoch = m.sizes.epoch();
    let owner = m.pane_owner();
    let generation = m.generation;
    let selection = m.selection.clone();
    m.action(Action::PaneFilter(id, rev + 1, true));
    assert_eq!(m.dual.other.as_ref().unwrap().query, "same");
    m.action(Action::PaneFilter(id, rev, true));
    let b = m.dual.other.as_ref().unwrap();
    assert!(b.query.is_empty() && b.selection.is_empty() && b.selected.is_none());
    assert_eq!(b.order.len(), 3);
    assert_eq!(m.query, "other");
    assert_eq!(m.selection, selection);
    assert_eq!(m.pane_owner(), owner);
    assert_eq!(m.generation, generation);
    assert_eq!(m.sizes.epoch(), epoch);
    assert_eq!(m.sizes.measured(&m.path, OsStr::new("folder")), Some(9));
    m.dual.other.as_mut().unwrap().refresh_selection =
        Some(crate::refresh_selection::Snapshot::capture(std::iter::empty(), None).unwrap());
    m.action(Action::PaneFilter(id, rev, false));
    assert_eq!(m.pane_owner(), owner);
    m.dual.other.as_mut().unwrap().refresh_selection = None;
    m.action(Action::PaneFilter(id, rev, false));
    assert_eq!(m.dual.active_id, id);
    assert!(m.filter_edit.active);
}
#[test]
fn per_pane_filter_controls_stay_disjoint_and_visible_at_440() {
    let f = Fixture::new();
    let mut m = f.model();
    m.action(Action::DualPane);
    ready(&mut m);
    m.query = "a long query that must not cover the buttons".into();
    m.dual.other.as_mut().unwrap().query = m.query.clone();
    for width in [356, 916] {
        let mut p = Recorder;
        m.hits.clear();
        let area = Rect::new(68, 94, width, 516);
        crate::dual_view::paint(&mut p, &mut m, area);
        let hits = m
            .hits
            .iter()
            .filter(|h| matches!(h.action, Action::PaneFilter(..)))
            .collect::<Vec<_>>();
        assert_eq!(hits.len(), 4);
        for (i, a) in hits.iter().enumerate() {
            assert!(a.rect.x >= area.x && a.rect.x + a.rect.w <= area.x + area.w);
            assert!(a.action.accessible_name().is_some());
            for b in hits.iter().skip(i + 1) {
                assert!(a.rect.x + a.rect.w <= b.rect.x || b.rect.x + b.rect.w <= a.rect.x);
            }
        }
    }
}
