use crate::{
    aux_io::AuxIo,
    model::{Action, Hit, Model},
    painter::Rect,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
};
struct Fixture {
    root: PathBuf,
    m: Model,
}
impl Fixture {
    fn new() -> Self {
        static ID: AtomicU64 = AtomicU64::new(0);
        let root = files_core::theme::home()
            .join(".cache/files-test")
            .join(format!(
                "nav-model-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        let mut m = Model::new(root.clone());
        let (tx, _) = mpsc::sync_channel(4);
        let (_, rx) = mpsc::channel();
        m.aux = AuxIo { tx, rx };
        Self { root, m }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn named(name: &str, kind: files_core::listing::Kind) -> files_core::listing::Entry {
    files_core::listing::Entry {
        name: name.into(),
        kind,
        metadata: None,
        stat_attempted: false,
    }
}
#[test]
fn combined_filter_prunes_operation_selection_and_matches_column_policy() {
    use files_core::listing::Kind;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.entries = vec![
        named("final-2026.rs", Kind::File),
        named("draft-2026.rs", Kind::File),
        named("final-2026.pdf", Kind::File),
        named(".final-2026.rs", Kind::File),
        named("source", Kind::Directory),
    ];
    m.selection.extend(0..5);
    m.selected = Some(1);
    m.anchor = Some(1);
    crate::input(m, "2026 ext:rs !draft type:file");
    assert_eq!(m.order, [0]);
    assert_eq!(m.selection.iter().copied().collect::<Vec<_>>(), [0]);
    assert_eq!(m.selected, Some(0));
    assert_eq!(m.anchor, None);
    for e in &m.entries {
        assert_eq!(
            crate::columns::accepts(e, false, &m.query),
            e.name == "final-2026.rs"
        );
    }
    m.open_copy();
    assert_eq!(
        m.copy_form.as_ref().unwrap().names.as_ref(),
        &[std::ffi::OsString::from("final-2026.rs")]
    );
    m.cancel_copy_dialog();
    m.action(Action::ClearFilter);
    assert_eq!(m.order.len(), 4);
    assert!(m.query.is_empty());
}
#[test]
fn insert_preserves_marked_batch_while_focus_advances_and_handles_last_row() {
    use files_core::listing::Kind;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.entries = vec![
        named("a", Kind::File),
        named("b", Kind::File),
        named("c", Kind::File),
    ];
    m.rebuild();
    m.selection.clear();
    m.mark_and_advance();
    assert_eq!(m.selected, Some(1));
    assert_eq!(m.selection.len(), 1);
    assert!(m.selection.contains(&0));
    m.mark_and_advance();
    assert_eq!(m.selected, Some(2));
    assert_eq!(m.selection.len(), 2);
    m.mark_and_advance();
    assert_eq!(m.selected, Some(2));
    assert_eq!(m.selection.len(), 3);
    m.mark_and_advance();
    assert_eq!(m.selected, Some(2));
    assert_eq!(m.selection.len(), 2);
}
#[test]
fn filter_action_leaves_path_editor_and_treemap_for_local_results() {
    let mut f = Fixture::new();
    f.m.editing = true;
    f.m.query = "old".into();
    f.m.view = crate::model::View::SpaceMap;
    f.m.action(Action::Filter);
    assert!(!f.m.editing);
    assert_eq!(f.m.query, "old");
    assert!(f.m.filter_edit.active);
    assert_eq!(f.m.filter_edit.cursor.range(&f.m.query), 0..3);
    assert!(f.m.view == crate::model::View::List);
    f.m.end_filter_edit();
    f.m.view = crate::model::View::SpaceMap;
    f.m.filter_input("typed").unwrap();
    assert!(f.m.view == crate::model::View::List && f.m.filter_edit.active);
}
#[test]
fn rapid_history_navigation_clears_targets_before_draw_and_branches() {
    let mut f = Fixture::new();
    let root = f.root.clone();
    let a = root.join("a");
    let b = root.join("b");
    for p in [&a, &b] {
        std::fs::create_dir(p).unwrap();
    }
    let m = &mut f.m;
    let old = m.generation;
    m.hits.push(Hit {
        rect: Rect::new(0, 0, 30, 30),
        generation: old,
        action: Action::Entry(999),
    });
    m.visible.push(999);
    m.selected = Some(999);
    m.navigate(a.clone());
    m.navigate(b.clone());
    m.travel_history(false);
    assert_eq!(m.path, a);
    assert_eq!(m.generation, old + 3);
    assert!(m.hits.is_empty() && m.visible.is_empty() && m.selected.is_none());
    assert!(m.history.can_back() && m.history.can_forward());
    m.travel_history(true);
    assert_eq!(m.path, b);
    m.travel_history(false);
    m.navigate(root.clone());
    assert_eq!(m.path, root);
    assert!(!m.history.can_forward());
}
#[test]
fn save_rejection_and_path_preflight_do_not_advance_history() {
    let mut f = Fixture::new();
    let original = f.m.path.clone();
    f.m.layout_dirty = true;
    f.m.layout_ready = true;
    f.m.layout_path = None;
    f.m.navigate(original.join("b"));
    assert_eq!(f.m.path, original);
    assert!(!f.m.history.can_back());
    f.m.layout_dirty = false;
    f.m.navigate(PathBuf::from("relative"));
    assert_eq!(f.m.path, original);
    assert!(!f.m.history.can_back());
    assert!(!f.m.error.is_empty());
}
#[test]
fn omnibar_rejected_replace_retains_text_flag_and_recovers() {
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.editing = true;
    m.edit = "old".into();
    m.replace_edit = true;
    crate::input(m, &"x".repeat(1024 * 1024));
    assert_eq!(m.edit, "old");
    assert!(m.replace_edit);
    assert_eq!(m.error, crate::omnibar_input::TOO_LONG);
    crate::input(m, "á\r\n😀");
    assert_eq!(m.edit, "á😀");
    assert!(!m.replace_edit);
    assert!(m.error.is_empty());
}

#[test]
fn search_modal_blocks_underlying_file_actions_and_stale_text_paste() {
    let mut f = Fixture::new();
    f.m.action(Action::Search);
    assert!(crate::native_drop::blocked(&f.m));
    assert!(crate::window_chrome::blocked(&f.m));
    let context = f.m.text_context();
    assert!(context.is_some());
    assert!(f.m.text_paste.request(context).is_some());
    f.m.search.input("new query");
    assert!(!f.m.receive_text_paste("late old callback"));
    assert_eq!(f.m.search.query, "new query");
    f.m.action(Action::SearchClose);
    assert!(f.m.search.is_drained());
}
#[test]
fn locating_same_folder_result_beats_refresh_snapshot_and_keeps_raw_name() {
    use files_core::{
        listing::Kind,
        search::{Hit, Report},
    };
    use std::os::unix::ffi::OsStringExt;
    let mut f = Fixture::new();
    let raw = std::ffi::OsString::from_vec(b"needle-\xff.rs".to_vec());
    std::fs::write(f.root.join("alpha.txt"), b"alpha").unwrap();
    std::fs::write(f.root.join(&raw), b"raw").unwrap();
    fn loaded(m: &mut Model) {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !m.loaded {
            m.pump();
            assert!(std::time::Instant::now() < end, "listing did not finish");
            std::thread::yield_now();
        }
    }
    f.m.refresh_current();
    loaded(&mut f.m);
    f.m.query = "alpha".into();
    f.m.rebuild();
    f.m.action(Action::Search);
    f.m.search.report = Some(Report {
        complete: true,
        hits: vec![Hit {
            relative: std::path::PathBuf::from(&raw),
            kind: Kind::File,
            content: None,
            snippet: None,
        }],
        ..Default::default()
    });
    f.m.action(Action::SearchOpen(true));
    loaded(&mut f.m);
    assert!(!f.m.search.visible);
    assert!(f.m.query.is_empty());
    assert_eq!(f.m.entries[f.m.selected.unwrap()].name, raw);
    assert!(f.m.search_reveal);
    assert_eq!(f.m.selection.len(), 1);
}

#[test]
fn measured_folder_sort_keeps_unknown_last_and_refresh_invalidates_observations() {
    use files_core::listing::{Kind, Sort};
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.entries = vec![
        named("large", Kind::Directory),
        named("small", Kind::Directory),
        named("unknown", Kind::Directory),
    ];
    m.rebuild();
    m.sizes
        .enqueue(
            &m.path,
            [std::ffi::OsStr::new("large"), std::ffi::OsStr::new("small")].into_iter(),
        )
        .unwrap();
    m.sizes.dispatch();
    m.sizes.finish(Ok((20, 3)));
    m.sizes.dispatch();
    m.sizes.finish(Ok((8, 2)));
    m.sort = Sort::Size;
    m.descending = false;
    m.rebuild();
    assert_eq!(m.order, [1, 0, 2]);
    m.descending = true;
    m.rebuild();
    assert_eq!(m.order, [0, 1, 2]);
    m.loaded = true;
    m.select_entry(0, false, false);
    m.action(Action::Sizes);
    let stamp = m.size_panel.as_ref().unwrap().stamp;
    let id = m.sizes.rows(&m.path)[0].id;
    m.action(Action::SizePanel(
        stamp,
        crate::size_panel::Command::Recalculate(id),
    ));
    assert_eq!(m.order, [1, 0, 2]); // recalculating is unknown, never the retained old total
    assert_eq!(m.selected, Some(0));
    m.action(Action::SizePanel(stamp, crate::size_panel::Command::Close));
    m.refresh_current();
    assert!(
        m.sizes
            .label(&m.path, std::ffi::OsStr::new("large"))
            .is_none()
    );
}
#[test]
fn listed_sizes_capture_filtered_names_and_focus_change_does_not_redirect_queue() {
    use files_core::listing::Kind;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.entries = vec![
        named("one", Kind::Directory),
        named("two", Kind::Directory),
        named(".hidden", Kind::Directory),
        named("file.txt", Kind::File),
    ];
    m.rebuild();
    m.action(Action::CalculateListedSizes);
    let first = m.sizes.dispatch().unwrap();
    m.select_entry(1, false, false);
    m.sizes.finish(Ok((3, 1)));
    assert_eq!(first, m.path.join("one"));
    assert_eq!(m.sizes.dispatch().unwrap(), m.path.join("two"));
    m.sizes.finish(Ok((8, 2)));
    assert!(m.sizes.dispatch().is_none());
    assert!(
        m.sizes
            .label(&m.path, std::ffi::OsStr::new(".hidden"))
            .is_none()
    );
}

#[test]
fn pattern_marking_returns_to_all_names_preserves_raw_and_copy_payload_exactly() {
    use crate::selection_tools::Command;
    use files_core::listing::Kind;
    use std::os::unix::ffi::OsStringExt;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    m.entries = vec![
        named("a.rs", Kind::File),
        named("b.txt", Kind::File),
        named(".hide.rs", Kind::File),
        named("raw", Kind::File),
    ];
    m.entries[3].name = std::ffi::OsString::from_vec(b"raw-\xff.rs".to_vec());
    m.query = "ext:rs type:file".into();
    m.rebuild();
    m.selected = Some(3);
    m.action(Action::Selection(Command::Filtered));
    assert!(m.query.is_empty());
    assert_eq!(m.order.len(), 3);
    assert_eq!(m.selection, [0, 3].into_iter().collect());
    assert_eq!(m.selected, Some(3));
    m.open_copy();
    let names = &m.copy_form.as_ref().unwrap().names;
    assert_eq!(
        names.as_ref(),
        [m.entries[0].name.clone(), m.entries[3].name.clone()]
    );
}
#[test]
fn selection_clear_and_empty_inversion_have_no_fallback_and_survive_rebuild() {
    use crate::selection_tools::Command;
    use files_core::listing::Kind;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    m.entries = vec![
        named("b", Kind::File),
        named("a", Kind::File),
        named(".hidden", Kind::File),
    ];
    m.rebuild();
    m.selected = Some(0);
    m.action(Action::Selection(Command::All));
    assert_eq!(m.selected, Some(0)); // Focus stays put, even when sorted before it.
    m.action(Action::Selection(Command::Invert));
    assert!(m.selection.is_empty() && m.selected.is_none() && m.anchor.is_none());
    m.rebuild();
    assert!(m.selection.is_empty() && m.selected.is_none());
    m.open_copy();
    assert!(m.copy_form.is_none());
    m.action(Action::Selection(Command::All));
    m.action(Action::Selection(Command::Clear));
    m.rebuild();
    assert!(m.selection.is_empty() && m.selected.is_none());
}
#[test]
fn invalid_pattern_and_partial_reload_leave_selection_untouched() {
    use crate::selection_tools::Command;
    use files_core::listing::Kind;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    m.entries = vec![named("a", Kind::File)];
    m.rebuild();
    m.query = "type:invalid".into();
    m.action(Action::Selection(Command::Filtered));
    assert_eq!(m.selection, [0].into_iter().collect());
    assert_eq!(m.query, "type:invalid");
    m.query.clear();
    m.loaded = false;
    m.action(Action::Selection(Command::Clear));
    assert_eq!(m.selection, [0].into_iter().collect());
    m.loaded = true;
    m.query = "no-match".into();
    m.rebuild();
    m.action(Action::Selection(Command::Filtered));
    assert!(m.query.is_empty() && m.selection.is_empty() && m.selected.is_none());
    m.rebuild();
    assert!(m.selection.is_empty() && m.selected.is_none());
}
#[test]
fn background_menu_keeps_selection_action_results_and_compact_selection_menu_fits() {
    use crate::selection_tools::Command;
    use files_core::listing::Kind;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    m.entries = vec![named("a", Kind::File), named("b", Kind::File)];
    m.rebuild();
    assert!(crate::context_menu::open_at(m, 400, 400, 800, 440));
    let index = m
        .context_menu
        .as_ref()
        .unwrap()
        .items
        .iter()
        .position(|item| matches!(item.action, Some(Action::Selection(Command::All))))
        .unwrap();
    crate::context_menu::activate(m, index);
    assert_eq!(m.selection.len(), 2);
    m.action(Action::SelectionMenu(440, 440));
    let menu = m.context_menu.as_ref().unwrap();
    assert_eq!(menu.target, crate::context_menu::Target::Selection);
    assert_eq!(menu.items.len(), 4);
    assert!(
        menu.rect.x >= 0
            && menu.rect.y >= 0
            && menu.rect.x + menu.rect.w <= 440
            && menu.rect.y + menu.rect.h <= 440
    );
}

#[test]
fn keyboard_ranges_focus_only_and_true_empty_navigation_work_in_visible_order() {
    use files_core::listing::Kind;
    use fltk::enums::Key;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.view = crate::model::View::List;
    m.loaded = true;
    m.entries = (0..30)
        .map(|n| named(&format!("f{n:02}.rs"), Kind::File))
        .collect();
    m.entries.push(named(".hidden", Kind::File));
    m.rebuild();
    crate::keyboard_nav::key(m, Key::Home, false, false, 1000, 650);
    crate::keyboard_nav::key(m, Key::Down, false, true, 1000, 650);
    crate::keyboard_nav::key(m, Key::Down, false, true, 1000, 650);
    assert_eq!(m.selection, [0, 1, 2].into_iter().collect());
    assert_eq!(m.anchor, Some(0));
    crate::keyboard_nav::key(m, Key::Up, false, true, 1000, 650);
    assert_eq!(m.selection, [0, 1].into_iter().collect());
    crate::keyboard_nav::key(m, Key::End, true, false, 1000, 650);
    assert_eq!(m.selection, [0, 1].into_iter().collect());
    assert_eq!(m.selected, Some(29));
    assert!(m.scroll > 0);
    crate::keyboard_nav::key(m, Key::Home, false, true, 1000, 650);
    assert_eq!(m.selection.len(), 30); // Range starts at the independently moved focus.

    m.action(Action::Selection(crate::selection_tools::Command::Clear));
    crate::keyboard_nav::key(m, Key::Down, false, true, 1000, 650);
    assert_eq!(m.selected, Some(0));
    assert_eq!(m.selection, [0].into_iter().collect());
    m.action(Action::Selection(crate::selection_tools::Command::Clear));
    crate::keyboard_nav::key(m, Key::Up, false, false, 1000, 650);
    assert_eq!(m.selected, Some(29));
    assert_eq!(m.selection, [29].into_iter().collect());
    m.editing = true;
    let old = m.selected;
    let scroll = m.scroll;
    crate::keyboard_nav::key(m, Key::PageDown, false, true, 1000, 650);
    assert_eq!(m.selected, old);
    assert_eq!(m.scroll, scroll);
}
#[test]
fn groups_keyboard_order_matches_painted_age_sections_and_folder_boundary() {
    use files_core::listing::{Kind, Metadata};
    use fltk::enums::Key;
    use std::time::{Duration, SystemTime};
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    let now = SystemTime::now();
    m.entries = vec![
        named("old-a", Kind::File),
        named("today-z", Kind::File),
        named("folder", Kind::Directory),
        named("week-m", Kind::File),
        named("unknown-b", Kind::File),
    ];
    for (index, age) in [(0, 20), (1, 0), (3, 2)] {
        m.entries[index].metadata = Some(Metadata {
            size: 1,
            modified: now - Duration::from_secs(age * 86400),
        });
    }
    m.rebuild();
    assert_eq!(m.order, [2, 1, 3, 0, 4]);
    crate::keyboard_nav::key(m, Key::Home, false, false, 1000, 650);
    assert_eq!(m.selected, Some(2));
    crate::keyboard_nav::key(m, Key::Down, false, true, 1000, 650);
    assert_eq!(m.selected, Some(1));
    assert_eq!(m.selection, [2, 1].into_iter().collect());
    crate::keyboard_nav::key(m, Key::End, false, true, 1000, 650);
    assert_eq!(m.selection.len(), 5);
    m.dual.enabled = true;
    m.rebuild();
    assert_eq!(m.order, [2, 0, 1, 4, 3]); // Dual paints ordinary name-sorted lists.
    m.view = crate::model::View::Grid;
    crate::keyboard_nav::key(m, Key::Home, false, false, 1000, 650);
    crate::keyboard_nav::key(m, Key::Down, false, false, 1000, 650);
    assert_eq!(m.selected, Some(0)); // Dual uses one list row, not Grid's column count.
}
#[test]
fn ctrl_shift_adds_range_and_reveal_does_not_accept_a_partially_clipped_row() {
    use files_core::listing::Kind;
    use fltk::enums::Key;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.view = crate::model::View::List;
    m.entries = (0..30)
        .map(|i| named(&format!("item-{i:02}"), Kind::File))
        .collect();
    m.rebuild();
    m.selection = [0, 29].into_iter().collect();
    crate::keyboard_nav::key(m, Key::Home, true, false, 800, 440);
    crate::keyboard_nav::key(m, Key::Down, true, true, 800, 440);
    assert_eq!(m.selection, [0, 1, 29].into_iter().collect());
    m.selected = Some(7);
    m.scroll = 0;
    m.visible = vec![7];
    crate::view::reveal_selection(m, 800, 440);
    let top = 30 + 7 * 38 - m.scroll;
    assert!(top >= 0 && top + 38 <= 440 - 134);
    m.anchor = Some(99);
    m.selected = Some(0);
    crate::keyboard_nav::key(m, Key::Down, false, true, 800, 440);
    assert_eq!(m.selection, [0, 1].into_iter().collect());
}
#[test]
fn path_uri_roundtrip_does_not_turn_literal_scheme_inside_absolute_path_into_remote() {
    use crate::path_edit::destination;
    assert_eq!(
        destination("/base/literal://name", std::path::Path::new("/base")).unwrap(),
        std::path::Path::new("/base/literal://name")
    );
    assert!(destination("file://", std::path::Path::new("/base")).is_err());
}

#[test]
fn guided_search_focus_rejects_late_paste_without_redirecting_text() {
    let mut f = Fixture::new();
    f.m.action(Action::Search);
    let context = f.m.text_context();
    assert!(f.m.text_paste.request(context).is_some());
    f.m.action(Action::SearchFilters);
    f.m.action(Action::SearchField(2));
    assert!(!f.m.receive_text_paste("late"));
    assert!(f.m.search.query.is_empty() && f.m.search.fields.iter().all(String::is_empty));
    let context = f.m.text_context();
    assert!(f.m.text_paste.request(context).is_some());
    assert!(f.m.receive_text_paste("10MiB"));
    assert_eq!(f.m.search.fields[1], "10MiB");
    assert!(f.m.search.query.is_empty());
}

#[test]
fn enter_during_current_search_does_not_restart_but_enter_after_cancel_does() {
    use fltk::enums::{Key, Shortcut};
    let mut f = Fixture::new();
    f.m.action(Action::Search);
    f.m.search.input("needle");
    f.m.search.start();
    let epoch = f.m.search.input_revision();
    crate::search_ui::key(&mut f.m, Key::Enter, Shortcut::None, "", 800, 440);
    assert_eq!(f.m.search.input_revision(), epoch);
    assert!(f.m.search.searching_current());
    f.m.search.cancel();
    crate::search_ui::key(&mut f.m, Key::Enter, Shortcut::None, "", 800, 440);
    assert_ne!(f.m.search.input_revision(), epoch);
    assert!(f.m.search.searching_current());
}

#[test]
fn size_control_actions_preserve_focus_results_and_pending_source() {
    use files_core::listing::Kind;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.entries = vec![named("one", Kind::Directory), named("two", Kind::Directory)];
    m.rebuild();
    m.action(Action::CalculateListedSizes);
    m.sizes.dispatch().unwrap();
    m.sizes.finish(Err("changed".into()));
    m.select_entry(1, false, false);
    m.action(Action::PauseSizes(true));
    m.action(Action::RetrySizeErrors);
    assert_eq!(m.selected, Some(1));
    assert_eq!(m.du_requested, Some(m.path.join("one")));
    assert!(m.sizes.dispatch().is_none());
    m.action(Action::PauseSizes(false));
    assert_eq!(m.sizes.dispatch().unwrap(), m.path.join("one"));
    m.sizes.finish(Ok((8, 2)));
    m.action(Action::CancelListedSizes);
    assert_eq!(
        m.sizes.measured(&m.path, std::ffi::OsStr::new("one")),
        Some(8)
    );
    assert!(!m.sizes.has_work());
}

#[test]
fn search_caret_and_result_focus_reject_late_text_paste_without_file_actions() {
    use crate::line_edit::Motion;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.action(Action::Search);
    m.search.input("日本.rs");
    let key = m.text_context();
    assert!(m.text_paste.request(key).is_some());
    m.search.motion(Motion::Start, false);
    assert!(!m.receive_text_paste("stale"));
    assert_eq!(m.search.query, "日本.rs");
    let key = m.text_context();
    assert!(m.text_paste.request(key).is_some());
    assert!(m.receive_text_paste("new-"));
    assert_eq!(m.search.query, "new-日本.rs");
    let key = m.text_context();
    assert!(m.text_paste.request(key).is_some());
    m.search.step(1);
    assert!(!m.receive_text_paste("stale-results"));
    assert!(m.copy_form.is_none() && m.rename_dialog.is_none());
}

#[test]
fn filter_focus_preserves_query_and_middle_edit_drives_live_results_and_text_shortcuts() {
    use crate::path_edit::KeyResult;
    use files_core::listing::Kind;
    use fltk::enums::{Key, Shortcut};
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.entries = vec![
        named("one.rs", Kind::File),
        named("two.rs", Kind::File),
        named("readme.md", Kind::File),
    ];
    m.loaded = true;
    m.query = "ext:rs".into();
    m.rebuild();
    let before = m.order.clone();
    m.action(Action::Filter);
    assert_eq!(m.query, "ext:rs");
    assert_eq!(m.order, before);
    m.filter_key(Key::End, Shortcut::None, "", 0);
    for _ in 0..2 {
        m.filter_key(Key::Left, Shortcut::Shift, "", 0);
    }
    m.filter_input("md").unwrap();
    assert_eq!(m.query, "ext:md");
    assert_eq!(m.order, [2]);
    m.filter_key(Key::from_char('z'), Shortcut::Ctrl, "", 0);
    assert_eq!(m.query, "ext:rs");
    assert_eq!(m.order, [0, 1]);
    m.filter_key(Key::from_char('z'), Shortcut::Ctrl | Shortcut::Shift, "", 0);
    assert_eq!(m.query, "ext:md");
    m.filter_key(Key::F5, Shortcut::None, "", 0);
    assert!(m.copy_form.is_none());
    m.filter_key(Key::End, Shortcut::None, "", 0);
    for _ in 0..2 {
        m.filter_key(Key::Left, Shortcut::Shift, "", 0);
    }
    assert!(
        matches!(m.filter_key(Key::from_char('c'),Shortcut::Ctrl,"",0),KeyResult::Copy(text) if text=="md")
    );
    assert_eq!(m.clipboard_state.count(), 0);
    m.filter_key(Key::Enter, Shortcut::None, "", 0);
    assert!(!m.filter_edit.active && m.query == "ext:md");
    m.action(Action::Filter);
    m.filter_key(Key::from_char('a'), Shortcut::Ctrl | Shortcut::Shift, "", 0);
    assert!(!m.filter_edit.active && m.query.is_empty());
    assert_eq!(m.selection, std::collections::HashSet::from([2]));
}
#[test]
fn filter_rejects_large_paste_atomically_and_stale_callbacks_after_cursor_or_exit() {
    use fltk::enums::{Key, Shortcut};
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.query = "keep".into();
    m.action(Action::Filter);
    let cap = m.query.capacity();
    let epoch = m.filter_edit.epoch;
    let caret = (m.filter_edit.cursor.caret, m.filter_edit.cursor.anchor);
    assert!(m.filter_input(&"x".repeat(1024 * 1024)).is_err());
    assert_eq!(m.query, "keep");
    assert_eq!(m.query.capacity(), cap);
    assert_eq!(m.filter_edit.epoch, epoch);
    assert_eq!(
        (m.filter_edit.cursor.caret, m.filter_edit.cursor.anchor),
        caret
    );
    let key = m.text_context();
    assert!(m.text_paste.request(key).is_some());
    m.filter_key(Key::Left, Shortcut::None, "", 0);
    assert!(!m.receive_text_paste("stale"));
    let key = m.text_context();
    assert!(m.text_paste.request(key).is_some());
    m.filter_key(Key::Enter, Shortcut::None, "", 0);
    assert!(!m.receive_text_paste("stale"));
    m.action(Action::Filter);
    assert_ne!(m.filter_edit.epoch, epoch);
    m.filter_key(Key::Home, Shortcut::None, "", 0);
    m.filter_input("ñ").unwrap();
    assert_eq!(m.query, "ñkeep");
}
#[test]
fn filter_navigation_and_path_editor_release_context_and_keep_query_persistence_rules() {
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.query = "*.rs".into();
    m.action(Action::Filter);
    m.action(Action::Omni);
    assert!(m.editing && !m.filter_edit.active);
    m.end_path_edit();
    m.action(Action::Filter);
    m.action(Action::Tab(crate::tabs::Command::New));
    assert!(!m.filter_edit.active);
    m.action(Action::Filter);
    m.load_location(m.path.join("other"));
    assert!(!m.filter_edit.active && m.query.is_empty());
}

#[test]
fn backspace_edits_focused_filter_but_navigates_parent_after_enter() {
    use fltk::enums::{Key, Shortcut};
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.query = "keep".into();
    m.action(Action::Filter);
    m.filter_key(Key::End, Shortcut::None, "", 0);
    m.filter_key(Key::BackSpace, Shortcut::None, "", 0);
    assert_eq!(m.query, "kee");
    assert_eq!(m.path, f.root);
    m.filter_key(Key::Enter, Shortcut::None, "", 0);
    crate::keyboard_nav::backspace(m);
    assert_eq!(m.path, f.root.parent().unwrap());
    assert!(m.query.is_empty() && !m.filter_edit.active);
    assert!(m.history.can_back());
}

#[test]
fn size_panel_owns_input_and_old_row_controls_cannot_cancel_a_retried_attempt() {
    use crate::size_panel::Command as C;
    use crate::size_queue::Status;
    use std::ffi::OsStr;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    m.sizes
        .enqueue(&m.path, [OsStr::new("folder")].into_iter())
        .unwrap();
    let old = m.sizes.rows(&m.path)[0].id;
    m.action(Action::Sizes);
    let stamp = m.size_panel.as_ref().unwrap().stamp;
    m.action(Action::SizePanel(stamp, C::Cancel(old)));
    assert!(matches!(m.sizes.rows(&m.path)[0].status, Status::Cancelled));
    m.action(Action::SizePanel(stamp, C::Retry(old)));
    let new = m.sizes.rows(&m.path)[0].id;
    assert_ne!(new, old);
    m.action(Action::SizePanel(stamp, C::Cancel(old)));
    assert!(matches!(m.sizes.rows(&m.path)[0].status, Status::Queued));
    let view = m.view;
    m.action(Action::View(crate::model::View::Grid));
    assert!(m.view == view);
    assert!(m.text_context().is_none() && crate::window_chrome::blocked(m));
    assert!(!crate::context_menu::open_at(m, 100, 150, 440, 440));
    m.action(Action::SizePanel(stamp, C::Close));
    assert!(m.size_panel.is_none() && m.sizes.has_work());
}
#[test]
fn size_panel_stale_owner_and_recapture_reject_old_window_commands() {
    use crate::size_panel::Command as C;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    m.sizes
        .enqueue(&m.path, [std::ffi::OsStr::new("folder")].into_iter())
        .unwrap();
    m.action(Action::Sizes);
    let old = m.size_panel.as_ref().unwrap().stamp;
    m.generation += 1;
    m.action(Action::SizePanel(old, C::Stop));
    assert!(m.sizes.pending());
    assert!(!m.size_panel.as_ref().unwrap().current(m));
    m.action(Action::SizePanel(old, C::Recapture));
    let new = m.size_panel.as_ref().unwrap().stamp;
    assert_ne!(new, old);
    m.action(Action::SizePanel(old, C::Stop));
    assert!(m.sizes.pending());
    m.action(Action::SizePanel(new, C::Stop));
    assert!(!m.sizes.pending());
}
fn loaded_bookmarks(m: &mut Model) -> u64 {
    m.loaded = true;
    m.action(Action::Bookmarks);
    let (tx, input) = mpsc::sync_channel(1);
    m.bookmarks.dispatch(&tx);
    let crate::aux_io::Request::Bookmarks(id, _) = input.try_recv().unwrap() else {
        panic!()
    };
    m.bookmarks.receive(
        id,
        Ok(files_core::bookmarks::Updated {
            document: files_core::bookmarks::Document {
                next: 2,
                rows: vec![files_core::bookmarks::Entry {
                    id: 1,
                    label: "Own".into(),
                    path: m.path.clone(),
                }],
            },
            unsynced: false,
            detail: None,
            notice: "Loaded",
        }),
    );
    m.bookmarks.panel.as_ref().unwrap().stamp
}
#[test]
fn bookmarks_modal_blocks_background_operations_and_stale_panel_or_owner() {
    use crate::bookmarks_ui::{Command, Row};
    let mut f = Fixture::new();
    let m = &mut f.m;
    let stamp = loaded_bookmarks(m);
    m.action(Action::CopyFiles);
    m.action(Action::NewEntry(true));
    assert!(m.copy_form.is_none() && m.rename_dialog.is_none());
    assert!(crate::native_drop::blocked(m));
    m.bookmark_command(stamp, Command::Select(Row::Favourite(1)));
    m.bookmark_command(stamp.wrapping_sub(1), Command::Remove);
    assert!(!m.bookmarks.busy());
    m.dual.active_id += 1;
    let stamp = m.bookmarks.panel.as_ref().unwrap().stamp;
    m.bookmark_command(stamp, Command::Remove);
    assert!(!m.bookmarks.busy());
    assert!(
        m.bookmarks
            .panel
            .as_ref()
            .unwrap()
            .local_error
            .contains("panel")
    );
    assert_eq!(m.bookmarks.document.rows.len(), 1);
}
#[test]
fn bookmark_edit_and_query_modes_reject_late_text_paste_without_folder_rename() {
    use crate::bookmarks_ui::{Command, Row};
    let mut f = Fixture::new();
    let m = &mut f.m;
    let stamp = loaded_bookmarks(m);
    let context = m.text_context();
    assert!(m.text_paste.request(context).is_some());
    m.bookmark_command(stamp, Command::Select(Row::Favourite(1)));
    assert!(!m.receive_text_paste("late query"));
    let stamp = m.bookmarks.panel.as_ref().unwrap().stamp;
    m.bookmark_command(stamp, Command::Edit);
    let context = m.text_context();
    assert!(m.text_paste.request(context).is_some());
    crate::bookmarks_ui::key(
        m,
        fltk::enums::Key::Escape,
        fltk::enums::Shortcut::None,
        "",
        800,
        440,
    );
    assert!(!m.receive_text_paste("late label"));
    assert!(m.rename_dialog.is_none());
    assert!(m.bookmarks.panel.as_ref().unwrap().editing.is_none());
    assert_eq!(m.bookmarks.document.rows[0].label, "Own");
    assert!(f.root.is_dir());
}

#[test]
fn bookmark_jump_to_current_path_reloads_instead_of_trusting_old_listing() {
    use crate::bookmarks_ui::{Command, Row};
    let mut f = Fixture::new();
    let m = &mut f.m;
    let stamp = loaded_bookmarks(m);
    m.bookmark_command(stamp, Command::Select(Row::Favourite(1)));
    let generation = m.generation;
    let path = m.path.clone();
    let stamp = m.bookmarks.panel.as_ref().unwrap().stamp;
    m.bookmark_command(stamp, Command::Jump);
    assert!(m.bookmarks.panel.is_none());
    assert_eq!(m.path, path);
    assert!(m.generation > generation);
    assert!(!m.loaded);
}

#[test]
fn bookmark_edit_invalidates_prior_controls_and_hub_shortcut_closes_without_add() {
    use crate::bookmarks_ui::{Command, Row};
    let mut f = Fixture::new();
    let m = &mut f.m;
    let stamp = loaded_bookmarks(m);
    m.bookmark_command(stamp, Command::Select(Row::Favourite(1)));
    let old = m.bookmarks.panel.as_ref().unwrap().stamp;
    m.bookmark_command(old, Command::Edit);
    assert_ne!(old, m.bookmarks.panel.as_ref().unwrap().stamp);
    m.bookmark_command(old, Command::Jump);
    assert!(m.bookmarks.panel.as_ref().unwrap().editing.is_some());
    crate::bookmarks_ui::key(
        m,
        fltk::enums::Key::from_char('d'),
        fltk::enums::Shortcut::Ctrl | fltk::enums::Shortcut::Alt,
        "",
        800,
        440,
    );
    assert!(m.bookmarks.panel.is_none());
    assert!(!m.bookmarks.busy());
    assert_eq!(m.bookmarks.document.rows.len(), 1);
}

#[test]
fn destination_paste_cannot_follow_changed_caret_or_reopened_dialog() {
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    m.entries = vec![named("note.txt", files_core::listing::Kind::File)];
    m.rebuild();
    m.select_entry(0, false, false);
    m.open_copy();
    let context = m.text_context();
    assert!(m.text_paste.request(context).is_some());
    crate::copy_ui::destination_key(m, fltk::enums::Key::End, fltk::enums::Shortcut::None, "");
    assert!(!m.receive_text_paste("late"));
    let old = m.text_context();
    assert!(m.text_paste.request(old).is_some());
    m.cancel_copy_dialog();
    m.open_copy();
    assert_ne!(old, m.text_context());
    assert!(!m.receive_text_paste("late"));
}

#[test]
fn copy_skip_policy_is_explicit_per_dialog_and_cannot_affect_move() {
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    m.entries = vec![named("note.txt", files_core::listing::Kind::File)];
    m.rebuild();
    m.select_entry(0, false, false);
    m.open_copy();
    assert_eq!(
        m.copy_form.as_ref().unwrap().existing_destination,
        files_core::copy_engine::ExistingDestination::Stop
    );
    crate::copy_ui::destination_key(m, fltk::enums::Key::F4, fltk::enums::Shortcut::None, "");
    assert_eq!(
        m.copy_form.as_ref().unwrap().existing_destination,
        files_core::copy_engine::ExistingDestination::Skip
    );
    m.action(Action::CopySkipExisting);
    assert_eq!(
        m.copy_form.as_ref().unwrap().existing_destination,
        files_core::copy_engine::ExistingDestination::KeepBoth
    );
    m.action(Action::CopySkipExisting);
    assert_eq!(
        m.copy_form.as_ref().unwrap().existing_destination,
        files_core::copy_engine::ExistingDestination::Stop
    );
    m.cancel_copy_dialog();
    m.open_copy();
    assert_eq!(
        m.copy_form.as_ref().unwrap().existing_destination,
        files_core::copy_engine::ExistingDestination::Stop
    );
    m.copy_form.as_mut().unwrap().moving = true;
    m.action(Action::CopySkipExisting);
    assert_eq!(
        m.copy_form.as_ref().unwrap().existing_destination,
        files_core::copy_engine::ExistingDestination::Stop
    );
}

#[test]
fn alternate_copy_name_focus_rows_and_old_controls_cannot_redirect_paste_or_source() {
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    m.entries = vec![
        named("a.txt", files_core::listing::Kind::File),
        named("b.txt", files_core::listing::Kind::File),
    ];
    m.rebuild();
    m.selection_command(crate::selection_tools::Command::All);
    m.open_copy();
    let parent = m.copy_form.as_ref().unwrap().source.clone();
    let names = m.copy_form.as_ref().unwrap().names.clone();
    let context = m.text_context();
    assert!(m.text_paste.request(context).is_some());
    crate::copy_ui::destination_key(m, fltk::enums::Key::F2, fltk::enums::Shortcut::None, "");
    assert!(!m.receive_text_paste("old-folder"));
    let context = m.text_context();
    assert!(m.text_paste.request(context).is_some());
    let epoch = m.copy_form.as_ref().unwrap().name_field.editor.epoch;
    m.action(Action::CopyNameStep(epoch, 1));
    assert!(!m.receive_text_paste("old-row"));
    let old = m.copy_form.as_ref().unwrap().name_field.editor.epoch;
    crate::copy_ui::destination_key(
        m,
        fltk::enums::Key::from_char('c'),
        fltk::enums::Shortcut::None,
        "new.txt",
    );
    m.action(Action::CopyNameReset(old));
    assert_eq!(m.copy_form.as_ref().unwrap().name_field.draft, "new.txt");
    let context = m.text_context();
    assert!(m.text_paste.request(context).is_some());
    crate::copy_ui::destination_key(
        m,
        fltk::enums::Key::from_char('l'),
        fltk::enums::Shortcut::Ctrl,
        "",
    );
    assert!(!m.receive_text_paste("old-name"));
    let d = m.copy_form.as_mut().unwrap();
    d.name_field.commit().unwrap();
    assert_eq!(d.source, parent);
    assert!(std::sync::Arc::ptr_eq(&d.names, &names));
    let paths = crate::copy_request_builder::build_named(
        &d.source,
        &d.destination_path(),
        &d.names,
        &d.name_field.plan,
        &files_core::copy_engine::CopyOptions::default(),
    )
    .unwrap();
    assert_eq!(paths[1].source.file_name().unwrap(), "b.txt");
    assert_eq!(paths[1].destination.file_name().unwrap(), "new.txt");
    assert!(m.rename_dialog.is_none());
}

#[test]
fn copy_drag_cannot_follow_an_inactive_field_after_keyboard_focus_change() {
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    m.entries = vec![named("a.txt", files_core::listing::Kind::File)];
    m.rebuild();
    m.select_entry(0, false, false);
    m.open_copy();
    {
        let d = m.copy_form.as_mut().unwrap();
        d.name_focus = true;
        d.editor.dragging = true;
    }
    assert!(!crate::copy_ui::destination_pointer(
        m, 0, 0, false, true, false, false
    ));
    {
        let d = m.copy_form.as_mut().unwrap();
        d.name_focus = false;
        d.editor.dragging = false;
        d.name_field.editor.dragging = true;
    }
    assert!(!crate::copy_ui::destination_pointer(
        m, 0, 0, false, true, false, false
    ));
}

#[test]
fn size_panel_sorted_navigation_keeps_record_identity_and_queue_order() {
    use crate::size_panel::Command as C;
    use crate::size_queue::Status;
    use std::ffi::OsStr;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    m.sizes
        .enqueue(
            &m.path,
            [OsStr::new("z"), OsStr::new("a"), OsStr::new("m")].into_iter(),
        )
        .unwrap();
    let ids: Vec<_> = m.sizes.rows(&m.path).iter().map(|r| r.id).collect();
    m.action(Action::Sizes);
    let stamp = m.size_panel.as_ref().unwrap().stamp;
    m.action(Action::SizePanel(stamp, C::Select(ids[2])));
    m.action(Action::SizePanel(stamp, C::Order));
    assert_eq!(m.size_panel.as_ref().unwrap().selected, 2);
    m.action(Action::SizePanel(stamp, C::Order));
    assert_eq!(m.size_panel.as_ref().unwrap().selected, 2);
    m.action(Action::SizePanel(stamp, C::Move(1)));
    assert_eq!(m.size_panel.as_ref().unwrap().selected, 0);
    crate::size_panel::key(m, fltk::enums::Key::Delete, fltk::enums::Shortcut::None);
    assert!(matches!(m.sizes.rows(&m.path)[0].status, Status::Cancelled));
    assert!(matches!(m.sizes.rows(&m.path)[1].status, Status::Queued));
    assert_eq!(
        m.sizes
            .rows(&m.path)
            .iter()
            .map(|r| r.id)
            .collect::<Vec<_>>(),
        ids
    );
    m.action(Action::SizePanel(stamp, C::Move(i32::MIN)));
    assert_eq!(m.size_panel.as_ref().unwrap().selected, 1);
    m.action(Action::SizePanel(stamp, C::Move(i32::MAX)));
    assert_eq!(m.size_panel.as_ref().unwrap().selected, 0);
}

#[test]
fn omnibar_catalog_prefix_opens_filtered_tool_without_executing_or_navigating() {
    let mut f = Fixture::new();
    let m = &mut f.m;
    let path = m.path.clone();
    let selection = m.selection.clone();
    m.begin_path_edit();
    m.path_input(">  rename  ").unwrap();
    let old = m.text_context();
    assert!(m.text_paste.request(old).is_some());
    m.commit_path_edit();
    assert!(!m.receive_text_paste("late path input"));
    assert!(!m.editing);
    assert_eq!(m.commands.as_ref().unwrap().query, "rename");
    assert!(m.rename_dialog.is_none());
    assert_eq!(m.path, path);
    assert_eq!(m.selection, selection);
    m.commands = None;
    m.begin_path_edit();
    m.path_input(": Proyectos ñ").unwrap();
    m.commit_path_edit();
    assert!(!m.editing);
    assert_eq!(m.bookmarks.panel.as_ref().unwrap().query, "Proyectos ñ");
    assert_eq!(m.path, path);
    assert_eq!(m.selection, selection);
}
#[test]
fn omnibar_catalog_rejects_oversize_and_controls_without_losing_draft() {
    let mut f = Fixture::new();
    let m = &mut f.m;
    for input in [format!(">{}", "ñ".repeat(129)), ": places\n".into()] {
        m.begin_path_edit();
        // Direct assignment covers hostile stored input, not merely text key filtering.
        m.edit = input.clone();
        m.commit_path_edit();
        assert!(m.editing && m.commands.is_none() && m.bookmarks.panel.is_none());
        assert_eq!(m.edit, input);
        assert!(m.error.starts_with("Ruta:"));
    }
    m.begin_path_edit();
    m.path_input(">").unwrap();
    m.commit_path_edit();
    assert!(m.commands.as_ref().is_some_and(|s| s.query.is_empty()));
}
#[test]
fn catalog_like_relative_names_remain_literal_with_dot_slash() {
    let current = std::path::Path::new("/fixture");
    assert_eq!(
        crate::path_edit::destination("./>rename", current).unwrap(),
        current.join("./>rename")
    );
    assert_eq!(
        crate::path_edit::destination("./:places", current).unwrap(),
        current.join("./:places")
    );
}

#[test]
fn size_panel_open_binds_selected_id_and_rejects_stale_context() {
    use crate::size_panel::Command as C;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    let root = m.path.clone();
    m.sizes
        .enqueue(
            &root,
            [std::ffi::OsStr::new("a"), std::ffi::OsStr::new("b")].into_iter(),
        )
        .unwrap();
    m.action(Action::Sizes);
    let stamp = m.size_panel.as_ref().unwrap().stamp;
    let ids: Vec<_> = m.sizes.rows(&root).iter().map(|r| r.id).collect();
    m.action(Action::SizePanel(stamp, C::Select(ids[1])));
    m.action(Action::SizePanel(stamp, C::Open(ids[0])));
    assert_eq!(m.path, root);
    assert!(m.size_panel.as_ref().unwrap().error.contains("fila cambió"));
    m.generation += 1;
    m.action(Action::SizePanel(stamp, C::Open(ids[1])));
    assert_eq!(m.path, root);
    assert!(!m.size_panel.as_ref().unwrap().current(m));
    m.action(Action::SizePanel(stamp, C::Recapture));
    let fresh = m.size_panel.as_ref().unwrap().stamp;
    m.action(Action::SizePanel(stamp, C::Open(ids[1])));
    assert_eq!(m.path, root);
    m.action(Action::SizePanel(fresh, C::Select(ids[1])));
    m.action(Action::SizePanel(fresh, C::Open(ids[1])));
    assert_eq!(m.path, root.join("b"));
    assert!(m.size_panel.is_none() && m.sizes.is_empty());
}

#[cfg(unix)]
#[test]
fn size_panel_alt_enter_opens_raw_selected_folder_without_mutating_paths() {
    use std::os::unix::ffi::OsStrExt;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    let root = m.path.clone();
    let name = std::ffi::OsStr::from_bytes(b"raw-\xff");
    m.sizes.enqueue(&root, [name].into_iter()).unwrap();
    m.action(Action::Sizes);
    crate::size_panel::key(m, fltk::enums::Key::Enter, fltk::enums::Shortcut::Alt);
    assert_eq!(m.path, root.join(name));
    assert!(m.size_panel.is_none() && m.sizes.is_empty());
}

#[test]
fn sftp_modal_rejects_file_actions_and_late_paste_across_field_and_reopen() {
    use crate::sftp_ui::Command as C;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    let path = m.path.clone();
    let view = m.view;
    m.action(Action::Sftp);
    let old = m.sftp.form.as_ref().unwrap().stamp;
    assert!(crate::native_drop::blocked(m) && crate::window_chrome::blocked(m));
    assert!(!crate::context_menu::open_at(m, 100, 150, 440, 440));
    m.action(Action::View(crate::model::View::Grid));
    m.action(Action::NewEntry(false));
    m.action(Action::Trash);
    assert!(m.view == view);
    assert!(m.rename_dialog.is_none());
    assert_eq!(m.path, path);
    assert!(m.text_paste.request(m.text_context()).is_some());
    m.action(Action::SftpCommand(old, C::Focus(3)));
    assert!(!m.receive_text_paste("late old field"));
    assert!(m.text_paste.request(m.text_context()).is_some());
    m.action(Action::SftpCommand(old, C::Close));
    m.action(Action::Sftp);
    let fresh = m.sftp.form.as_ref().unwrap().stamp;
    assert_ne!(old, fresh);
    assert!(!m.receive_text_paste("late old form"));
    m.action(Action::SftpCommand(old, C::Close));
    assert!(m.sftp.form.is_some());
    m.sftp_input("Work-Alias");
    m.action(Action::SftpCommand(fresh, C::Review));
    assert_eq!(
        m.sftp
            .form
            .as_ref()
            .unwrap()
            .review
            .as_ref()
            .unwrap()
            .source,
        "Work-Alias:"
    );
    assert_eq!(m.path, path);
}
#[test]
fn sftp_field_shortcuts_cursor_and_rejected_port_keep_the_requested_source_literal() {
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    m.action(Action::Sftp);
    m.sftp_input("host");
    crate::sftp_ui::key(
        m,
        fltk::enums::Key::from_char('4'),
        fltk::enums::Shortcut::Ctrl,
        "",
    );
    m.sftp_input("/Fotos ñ/%20/$()");
    crate::sftp_ui::key(m, fltk::enums::Key::Left, fltk::enums::Shortcut::None, "");
    let before = m.sftp.form.as_ref().unwrap().fields[3].clone();
    let cursor = m.sftp.form.as_ref().unwrap().cursors[3].caret;
    m.sftp_input(&"x".repeat(1024 * 1024));
    assert_eq!(m.sftp.form.as_ref().unwrap().fields[3], before);
    assert_eq!(m.sftp.form.as_ref().unwrap().cursors[3].caret, cursor);
    crate::sftp_ui::key(m, fltk::enums::Key::Enter, fltk::enums::Shortcut::None, "");
    assert_eq!(
        m.sftp
            .form
            .as_ref()
            .unwrap()
            .review
            .as_ref()
            .unwrap()
            .source,
        "host:/Fotos ñ/%20/$()"
    );
    crate::sftp_ui::key(
        m,
        fltk::enums::Key::from_char('3'),
        fltk::enums::Shortcut::Ctrl,
        "",
    );
    m.sftp_input("65536");
    crate::sftp_ui::key(m, fltk::enums::Key::Enter, fltk::enums::Shortcut::None, "");
    assert!(m.sftp.form.as_ref().unwrap().review.is_none());
    assert!(m.sftp.form.as_ref().unwrap().error.contains("Puerto"));
}

#[test]
fn copy_options_are_explicit_modal_and_do_not_revive_old_paste_or_affect_move() {
    use files_core::copy_engine::Publication as P;
    use fltk::enums::{Key, Shortcut};
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    m.entries = vec![named("note.txt", files_core::listing::Kind::File)];
    m.rebuild();
    m.select_entry(0, false, false);
    m.open_copy();
    assert_eq!(m.copy_form.as_ref().unwrap().publication, P::StagedRename);
    assert!(m.copy_form.as_ref().unwrap().preserve_symlink_times);
    let context = m.text_context();
    m.text_paste.request(context).unwrap();
    let before = m.copy_form.as_ref().unwrap().destination.clone();
    crate::copy_ui::destination_key(m, Key::F3, Shortcut::None, "");
    assert!(m.copy_form.as_ref().unwrap().options_open);
    assert!(m.text_context().is_none());
    assert!(!m.receive_text_paste("OLD"));
    crate::copy_ui::destination_key(m, Key::None, Shortcut::None, "UNWANTED");
    assert_eq!(m.copy_form.as_ref().unwrap().destination, before);
    crate::copy_ui::destination_key(m, Key::from_char('1'), Shortcut::None, "");
    crate::copy_ui::destination_key(m, Key::from_char('2'), Shortcut::None, "");
    assert_eq!(
        m.copy_form.as_ref().unwrap().publication,
        P::VisibleExclusive
    );
    assert!(!m.copy_form.as_ref().unwrap().preserve_symlink_times);
    m.start_copy();
    assert!(m.copy_form.is_some() && !m.copy_busy());
    crate::copy_ui::destination_key(m, Key::Enter, Shortcut::None, "");
    assert!(!m.copy_form.as_ref().unwrap().options_open && !m.copy_busy());
    assert!(
        m.copy_form
            .as_ref()
            .unwrap()
            .policy_summary()
            .contains("conserva parciales")
    );
    m.cancel_copy_dialog();
    m.open_copy();
    assert_eq!(m.copy_form.as_ref().unwrap().publication, P::StagedRename);
    assert!(m.copy_form.as_ref().unwrap().preserve_symlink_times);
    m.copy_form.as_mut().unwrap().moving = true;
    m.action(Action::CopyOptions);
    m.action(Action::CopyVisiblePublication);
    m.action(Action::CopySymlinkTimes);
    assert!(!m.copy_form.as_ref().unwrap().options_open);
    assert_eq!(m.copy_form.as_ref().unwrap().publication, P::StagedRename);
    assert!(m.copy_form.as_ref().unwrap().preserve_symlink_times);
}

#[test]
fn search_calendar_keyboard_blocks_text_receipts_and_requires_explicit_search() {
    use fltk::enums::{Key, Shortcut};
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.search.open("/calendar".into(), "name".into(), false);
    let context = m.text_context();
    m.text_paste.request(context).unwrap();
    crate::search_ui::key(m, Key::F8, Shortcut::None, "", 440, 440);
    assert!(m.search.calendar_open && m.text_context().is_none());
    assert!(!m.receive_text_paste("STALE"));
    crate::search_ui::key(m, Key::None, Shortcut::None, "UNWANTED", 440, 440);
    assert_eq!(m.search.query, "name");
    crate::search_ui::key(m, Key::from_char('1'), Shortcut::None, "", 440, 440);
    assert!(m.search.date_busy() && !m.search.running());
    crate::search_ui::key(m, Key::Escape, Shortcut::None, "", 440, 440);
    assert!(!m.search.calendar_open && !m.search.date_busy() && !m.search.running());
    assert!(m.search.fields[2].is_empty());
}

#[test]
fn size_report_freezes_observation_until_explicit_refresh_and_guards_context() {
    use crate::size_panel::Command as C;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    m.sizes
        .enqueue(&m.path, [std::ffi::OsStr::new("empty")].into_iter())
        .unwrap();
    assert!(m.sizes.dispatch().is_some());
    m.action(Action::Sizes);
    let stamp = m.size_panel.as_ref().unwrap().stamp;
    m.action(Action::SizePanel(stamp, C::Report));
    let captured = crate::size_panel::fixture_state(m).unwrap();
    assert!(captured["summary"]["bytes"].is_null());
    m.sizes.finish(Ok((0, 0)));
    assert_eq!(
        crate::size_panel::fixture_state(m).unwrap()["report"],
        captured["report"]
    );
    // Direct stale/hypothetical queue actions are blocked while reading, too.
    m.action(Action::SizePanel(stamp, C::Listed));
    assert!(m.size_panel.as_ref().unwrap().error.contains("Vuelve"));
    m.action(Action::SizePanel(stamp, C::RefreshReport));
    let refreshed = crate::size_panel::fixture_state(m).unwrap();
    assert_eq!(refreshed["summary"]["bytes"], 0);
    assert_eq!(refreshed["summary"]["measured"], 1);
    m.generation += 1;
    m.action(Action::SizePanel(stamp, C::RefreshReport));
    assert_eq!(
        crate::size_panel::fixture_state(m).unwrap()["report"],
        refreshed["report"]
    );
    assert!(!m.size_panel.as_ref().unwrap().error.is_empty());
    crate::size_panel::key(m, fltk::enums::Key::Escape, fltk::enums::Shortcut::None);
    assert!(m.size_panel.is_none());
}

#[cfg(unix)]
#[test]
fn background_search_tab_keeps_search_listing_and_raw_focus_until_activation() {
    use files_core::{
        listing::Kind,
        search::{Hit, Report},
    };
    use std::os::unix::ffi::OsStringExt;
    let mut f = Fixture::new();
    let m = &mut f.m;
    let raw = std::ffi::OsString::from_vec(b"found-\xff.txt".to_vec());
    let parent = m.path.join("nested");
    std::fs::create_dir(&parent).unwrap();
    std::fs::write(parent.join(&raw), b"original").unwrap();
    m.query = "source filter".into();
    m.selected = Some(3);
    m.selection.insert(3);
    m.action(Action::Search);
    m.search.report = Some(Report {
        complete: true,
        hits: vec![Hit {
            relative: std::path::PathBuf::from("nested").join(&raw),
            kind: Kind::File,
            content: None,
            snippet: None,
        }],
        ..Default::default()
    });
    m.search.results_focus();
    let generation = m.generation;
    let source = m.path.clone();
    let active = m.tabs.active;
    crate::search_ui::key(
        m,
        fltk::enums::Key::Enter,
        fltk::enums::Shortcut::Ctrl,
        "",
        1000,
        650,
    );
    let id = m.search.background_tab.unwrap();
    assert_eq!(m.tabs.active, active);
    assert_eq!(m.generation, generation);
    assert_eq!(m.path, source);
    assert_eq!(m.query, "source filter");
    assert_eq!(m.selected, Some(3));
    assert!(m.selection.contains(&3));
    assert!(m.search.visible && m.search.report.as_ref().unwrap().complete);
    let saved = m.tabs.rows.last().unwrap().saved.as_ref().unwrap();
    assert_eq!(saved.path, parent);
    assert!(saved.selection.focused(&raw));
    assert!(saved.selection.contains(&raw) && saved.selection.anchored(&raw));
    assert!(saved.query.is_empty() && !saved.history.can_back() && !saved.history.can_forward());
    m.action(Action::Tab(crate::tabs::Command::Select(id)));
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !m.loaded {
        m.pump();
        assert!(std::time::Instant::now() < until);
        std::thread::yield_now();
    }
    assert!(!m.search.visible);
    assert_eq!(m.path, parent);
    assert_eq!(m.entries[m.selected.unwrap()].name, raw);
    assert!(m.selection.contains(&m.selected.unwrap()));
    m.action(Action::Tab(crate::tabs::Command::Select(active)));
    assert_eq!(m.path, source);
    assert_eq!(m.query, "source filter");
}

#[test]
fn background_tab_preflight_counts_other_pane_and_closed_state_without_partial_insertion() {
    let mut f = Fixture::new();
    let m = &mut f.m;
    let path = m.path.join("not-probed");
    assert!(
        m.background_tab(&path, Some(std::ffi::OsStr::new("../bad")))
            .is_err()
    );
    assert!(
        m.background_tab(std::path::Path::new(&"x".repeat(4097)), None)
            .is_err()
    );
    assert_eq!(m.tabs.rows.len(), 1);
    m.closing = true;
    assert!(m.background_tab(&path, None).is_err());
    m.closing = false;
    let mut ids = Vec::new();
    for _ in 1..crate::tabs::MAX_TABS {
        ids.push(m.background_tab(&path, None).unwrap());
    }
    assert!(ids.windows(2).all(|p| p[0] < p[1]));
    let charge = m.tabs.stored_bytes();
    assert!(m.background_tab(&path, None).unwrap_err().contains("ocho"));
    assert_eq!(m.tabs.stored_bytes(), charge);
    assert_eq!(m.tabs.active, 1);
    let closed = m.tabs.rows.pop().unwrap().saved.unwrap();
    m.tabs.closed.push(closed);
    m.tabs.closed.last_mut().unwrap().charge = crate::tabs::STORED_LIMIT;
    assert!(
        m.background_tab(&path, None)
            .unwrap_err()
            .contains("presupuesto")
    );
    assert_eq!(m.tabs.rows.len(), 7);
    m.tabs.closed.clear();
    m.toggle_dual();
    assert!(m.dual.other.is_some());
    let full = m.tabs.rows.pop().unwrap().saved.unwrap();
    m.dual.other.as_mut().unwrap().tabs.closed.push(full);
    m.dual
        .other
        .as_mut()
        .unwrap()
        .tabs
        .closed
        .last_mut()
        .unwrap()
        .charge = crate::tabs::STORED_LIMIT;
    let count = m.tabs.rows.len();
    assert!(
        m.background_tab(&path, None)
            .unwrap_err()
            .contains("presupuesto")
    );
    assert_eq!(m.tabs.rows.len(), count);
}

#[test]
fn search_background_directory_and_provisional_hit_do_not_require_or_restart_a_search() {
    use files_core::{listing::Kind, search::Hit};
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.action(Action::Search);
    m.search.provisional = Some(
        vec![Hit {
            relative: "missing-directory".into(),
            kind: Kind::Directory,
            content: None,
            snippet: None,
        }]
        .into(),
    );
    m.action(Action::SearchBackgroundTab);
    assert!(m.search.visible && m.search.provisional.is_some() && m.search.report.is_none());
    let saved = m.tabs.rows.last().unwrap().saved.as_ref().unwrap();
    assert_eq!(saved.path, m.path.join("missing-directory"));
    assert!(saved.selection.focus().is_none());
    assert!(saved.selection.names().next().is_none());
    m.action(Action::SearchSelect(0, m.search.result_epoch));
    assert!(m.search.background_tab.is_none());
    let tabs = m.tabs.rows.len();
    m.search.provisional = None;
    m.action(Action::SearchBackgroundTab);
    assert_eq!(m.tabs.rows.len(), tabs);
}

#[test]
fn search_marks_range_stale_hit_and_batch_tab_admission_are_atomic() {
    use files_core::{
        listing::Kind,
        search::{Hit, Report},
    };
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.action(Action::Search);
    m.search.report = Some(Report {
        complete: true,
        hits: (0..9)
            .map(|i| Hit {
                relative: format!("d{i}/f").into(),
                kind: Kind::File,
                content: None,
                snippet: None,
            })
            .collect(),
        ..Default::default()
    });
    let epoch = m.search.result_epoch;
    m.action(Action::SearchSelect(0, epoch));
    m.search.select_result(2, false, true, false);
    assert_eq!(m.search.marks.len(), 3);
    m.action(Action::SearchToggle(5, epoch + 1));
    assert_eq!(m.search.marks.len(), 3);
    assert_eq!(m.search.selected, 2);
    let active = m.tabs.active;
    let generation = m.generation;
    m.action(Action::SearchBackgroundTab);
    assert_eq!(m.tabs.rows.len(), 4);
    assert_eq!(m.search.background_count, 3);
    assert_eq!(m.tabs.active, active);
    assert_eq!(m.generation, generation);
    assert!(
        m.tabs.rows[1..]
            .iter()
            .enumerate()
            .all(|(i, t)| t.path == m.path.join(format!("d{i}")))
    );
    m.search.mark_all();
    let ids = m.tabs.rows.iter().map(|t| t.id).collect::<Vec<_>>();
    m.action(Action::SearchBackgroundTab);
    assert!(m.search.error.contains("lote"));
    assert_eq!(m.tabs.rows.iter().map(|t| t.id).collect::<Vec<_>>(), ids);
    m.search.marks.reconcile(&[]);
    assert!(m.search.marks.explicit);
    m.action(Action::SearchBackgroundTab);
    assert!(m.search.error.contains("No quedan"));
    assert_eq!(m.tabs.rows.len(), 4);
}

#[test]
fn background_batch_preflights_invalid_last_target_before_any_publication() {
    let mut f = Fixture::new();
    let m = &mut f.m;
    let valid = m.path.join("not-opened");
    let bad = std::path::PathBuf::from("x".repeat(4097));
    assert!(
        m.background_tabs([(valid.as_path(), None), (bad.as_path(), None)].into_iter())
            .is_err()
    );
    assert_eq!(m.tabs.rows.len(), 1);
    let id = m.background_tab(&valid, None).unwrap();
    assert_eq!(id, 2);
}

#[test]
fn marked_background_locations_follow_visual_order_after_name_sort() {
    use files_core::{
        listing::Kind,
        search::{Hit, Report},
    };
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.action(Action::Search);
    m.search.report = Some(Report {
        complete: true,
        hits: ["z/file2", "a/file10", "b/file1"]
            .iter()
            .map(|p| Hit {
                relative: p.into(),
                kind: Kind::File,
                content: None,
                snippet: None,
            })
            .collect(),
        ..Default::default()
    });
    m.search.mark_all();
    m.action(Action::SearchOrder);
    m.action(Action::SearchBackgroundTab);
    let paths = m.tabs.rows[1..]
        .iter()
        .map(|t| t.path.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [m.path.join("b"), m.path.join("z"), m.path.join("a")]
    );
}

#[test]
fn size_recalculation_binds_selected_attempt_context_and_does_not_change_captured_report() {
    use crate::size_panel::Command as C;
    use fltk::enums::{Key, Shortcut};
    use std::ffi::OsStr;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    m.sizes
        .enqueue(&m.path, [OsStr::new("a"), OsStr::new("b")].into_iter())
        .unwrap();
    m.sizes.dispatch().unwrap();
    m.sizes.finish(Ok((8, 1)));
    m.sizes.dispatch().unwrap();
    m.sizes.finish(Ok((9, 1)));
    let a = m.sizes.rows(&m.path)[0].id;
    let b = m.sizes.rows(&m.path)[1].id;
    m.action(Action::Sizes);
    let stamp = m.size_panel.as_ref().unwrap().stamp;
    m.action(Action::SizePanel(stamp, C::Select(b)));
    m.action(Action::SizePanel(stamp, C::Recalculate(a)));
    assert_eq!(m.sizes.measured(&m.path, OsStr::new("a")), Some(8));
    assert!(!m.sizes.has_work());
    m.action(Action::SizePanel(stamp, C::Report));
    let capture = crate::size_panel::fixture_state(m).unwrap()["report"]["text"].clone();
    m.action(Action::SizePanel(stamp, C::Recalculate(b)));
    assert_eq!(
        crate::size_panel::fixture_state(m).unwrap()["report"]["text"],
        capture
    );
    assert!(!m.sizes.has_work());
    m.action(Action::SizePanel(stamp, C::Report));
    crate::size_panel::key(m, Key::F5, Shortcut::None);
    let fresh = m.sizes.rows(&m.path)[1].id;
    assert_ne!(fresh, b);
    assert_eq!(m.size_panel.as_ref().unwrap().selected, 1);
    assert_eq!(m.sizes.measured(&m.path, OsStr::new("a")), Some(8));
    assert_eq!(m.sizes.measured(&m.path, OsStr::new("b")), None);
    m.action(Action::SizePanel(stamp, C::Recalculate(b)));
    assert_eq!(m.sizes.rows(&m.path)[1].id, fresh);
    m.generation += 1;
    m.action(Action::SizePanel(stamp, C::Recalculate(a)));
    assert_eq!(m.sizes.measured(&m.path, OsStr::new("a")), Some(8));
}

#[test]
fn late_search_stop_does_not_erase_a_completed_page_scope() {
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.search.open(m.path.clone(), "ext:txt".into(), false);
    m.search.page_mode = true;
    let page = files_core::search_page::Page {
        version: 1,
        id: 1,
        sequence: 0,
        found_total: 1,
        more: false,
        report: files_core::search::Report {
            hits: vec![files_core::search::Hit {
                relative: "file.txt".into(),
                kind: files_core::listing::Kind::File,
                content: None,
                snippet: None,
            }],
            visited: 1,
            complete: true,
            ..Default::default()
        },
    };
    m.search
        .pages
        .admit(page, &mut m.search.report, Default::default())
        .unwrap();
    m.action(Action::SearchCancel);
    assert!(m.search.pages.current.is_some_and(|p| p.sequence == 0));
    assert!(m.search.pages.complete && m.search.pages.found == 1 && m.search.hits().len() == 1);
}

#[test]
fn forgetting_measurement_keeps_browser_selection_generation_and_rejects_old_panel_buttons() {
    use crate::size_panel::Command as C;
    use files_core::listing::{Kind, Sort};
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.entries = vec![
        named("large", Kind::Directory),
        named("small", Kind::Directory),
        named("unknown", Kind::Directory),
    ];
    m.sizes
        .enqueue(
            &m.path,
            [std::ffi::OsStr::new("large"), std::ffi::OsStr::new("small")].into_iter(),
        )
        .unwrap();
    m.sizes.dispatch();
    m.sizes.finish(Ok((20, 1)));
    m.sizes.dispatch();
    m.sizes.finish(Ok((8, 1)));
    m.sort = Sort::Size;
    m.descending = true;
    m.rebuild();
    m.loaded = true;
    m.select_entry(0, false, false);
    let generation = m.generation;
    m.action(Action::Sizes);
    let stamp = m.size_panel.as_ref().unwrap().stamp;
    let id = m.sizes.rows(&m.path)[0].id;
    let keep = m.sizes.rows(&m.path)[1].id;
    m.action(Action::SizePanel(stamp, C::Forget(id)));
    assert_eq!(m.generation, generation);
    assert_eq!(m.selected, Some(0));
    assert!(m.selection.contains(&0));
    assert_eq!(m.order, [1, 0, 2]);
    assert_eq!(
        m.sizes.measured(&m.path, std::ffi::OsStr::new("large")),
        None
    );
    m.action(Action::SizePanel(stamp, C::Forget(keep)));
    assert_eq!(
        m.sizes.measured(&m.path, std::ffi::OsStr::new("small")),
        Some(8)
    );
}

#[test]
fn forget_report_uses_frozen_ids_and_keeps_measurements_that_finish_later() {
    use crate::size_panel::Command as C;
    use files_core::listing::Kind;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.entries = vec![
        named("old", Kind::Directory),
        named("later", Kind::Directory),
        named("pending", Kind::Directory),
    ];
    m.loaded = true;
    m.rebuild();
    m.sizes
        .enqueue(
            &m.path,
            [
                std::ffi::OsStr::new("old"),
                std::ffi::OsStr::new("later"),
                std::ffi::OsStr::new("pending"),
            ]
            .into_iter(),
        )
        .unwrap();
    m.sizes.dispatch();
    m.sizes.finish(Ok((0, 0)));
    let old = m.sizes.rows(&m.path)[0].id;
    m.action(Action::Sizes);
    let stamp = m.size_panel.as_ref().unwrap().stamp;
    m.action(Action::SizePanel(stamp, C::Report));
    m.sizes.dispatch();
    m.sizes.finish(Ok((9, 1)));
    m.sizes.pause(true);
    m.action(Action::SizePanel(
        stamp,
        C::ForgetMeasured(std::sync::Arc::from([old])),
    ));
    assert!(m.sizes.paused());
    assert_eq!(m.sizes.measured(&m.path, std::ffi::OsStr::new("old")), None);
    assert_eq!(
        m.sizes.measured(&m.path, std::ffi::OsStr::new("later")),
        Some(9)
    );
    let fresh = m.size_panel.as_ref().unwrap().stamp;
    let later = m.sizes.rows(&m.path)[0].id;
    m.action(Action::SizePanel(fresh, C::Report));
    m.action(Action::SizePanel(
        fresh,
        C::ForgetMeasured(std::sync::Arc::from([later])),
    ));
    assert_eq!(
        m.sizes.measured(&m.path, std::ffi::OsStr::new("later")),
        Some(9)
    );
}

#[test]
fn clearing_filter_after_explicit_selection_clear_does_not_capture_an_unchosen_file() {
    use crate::selection_tools::Command;
    use files_core::listing::Kind;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    m.view = crate::model::View::List;
    m.entries = vec![
        named("chosen.rs", Kind::File),
        named("other.txt", Kind::File),
    ];
    m.query = "*.rs".into();
    m.rebuild();
    m.action(Action::Selection(Command::Clear));
    assert!(m.selection.is_empty() && m.selected.is_none());
    m.action(Action::ClearFilter);
    assert_eq!(m.order.len(), 2);
    assert!(
        m.selection.is_empty() && m.selected.is_none(),
        "clearing a filter must not undo explicit zero selection"
    );
    m.open_copy();
    assert!(m.copy_form.is_none());
}

#[test]
fn explicit_zero_selection_survives_filter_escape_across_browser_views() {
    use crate::{model::View, selection_tools::Command};
    use files_core::listing::Kind;
    use fltk::enums::{Key, Shortcut};
    for view in [View::List, View::Grid, View::Groups, View::Columns] {
        let mut f = Fixture::new();
        let m = &mut f.m;
        m.loaded = true;
        m.view = view;
        m.entries = vec![named("one.rs", Kind::File), named("two.txt", Kind::File)];
        m.query = "*.rs".into();
        m.rebuild();
        m.action(Action::Selection(Command::Clear));
        assert!(m.begin_filter_edit(true));
        m.filter_key(Key::Escape, Shortcut::None, "", 0);
        assert!(
            m.query.is_empty() && m.selection.is_empty() && m.selected.is_none(),
            "view {}",
            view as u8
        );
        m.open_copy();
        assert!(m.copy_form.is_none(), "view {}", view as u8);
    }
}
#[test]
fn clearing_no_match_filter_still_restores_default_focus_without_explicit_clear() {
    use files_core::listing::Kind;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    m.entries = vec![named("one.rs", Kind::File)];
    m.query = "absent".into();
    m.rebuild();
    assert!(m.selected.is_none());
    m.action(Action::ClearFilter);
    assert_eq!(m.selected, Some(0));
    assert_eq!(m.selection, [0].into_iter().collect());
}

fn folder_default(m: &mut Model, path: PathBuf) {
    m.folder_views.loaded = true;
    m.folder_views.document = std::sync::Arc::new(files_core::folder_views::Document {
        rows: vec![files_core::folder_views::Entry {
            path,
            settings: files_core::folder_views::Settings {
                view: 3,
                sort: 1,
                descending: true,
                zoom: 125,
                scroll: 84,
            },
        }],
    });
}
#[test]
fn folder_view_waits_for_listing_then_restores_without_hidden_or_query_changes() {
    let mut f = Fixture::new();
    let m = &mut f.m;
    folder_default(m, f.root.clone());
    m.show_hidden = true;
    m.query = "kept".into();
    assert!(!m.pump_folder_restore());
    m.loaded = true;
    assert!(m.pump_folder_restore());
    assert!(m.view == crate::model::View::Grid);
    assert!(m.sort == files_core::listing::Sort::Size);
    assert!(m.descending && m.show_hidden);
    assert_eq!(m.query, "kept");
    assert_eq!(m.grid_zoom, 1.25);
    assert_eq!(m.scroll, 84);
    m.content_height = 150;
    m.clamp_scroll(100);
    assert_eq!(m.scroll, 50);
    assert!(!m.pump_folder_restore());
}
#[test]
fn folder_view_manual_intent_even_if_reverted_and_stale_generation_reject_late_read() {
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.action(Action::GridZoom(1.25));
    m.action(Action::GridZoom(0.8));
    folder_default(m, f.root.clone());
    m.loaded = true;
    assert!(!m.pump_folder_restore());
    assert!(m.view == crate::model::View::Groups);
    m.arm_folder_restore();
    m.generation += 1;
    assert!(!m.pump_folder_restore());
    m.arm_folder_restore();
    m.closing = true;
    assert!(!m.pump_folder_restore());
}
#[test]
fn folder_view_restore_keeps_dual_enabled_and_owner_switch_rejects() {
    let mut f = Fixture::new();
    let m = &mut f.m;
    folder_default(m, f.root.clone());
    m.loaded = true;
    m.toggle_dual();
    m.arm_folder_restore();
    assert!(m.pump_folder_restore());
    assert!(m.dual.enabled && m.view == crate::model::View::List);
    m.arm_folder_restore();
    m.dual.active_id += 1;
    assert!(!m.pump_folder_restore());
}
#[test]
fn folder_view_explicit_save_keeps_captured_path_through_navigation_and_forget_is_cas() {
    use crate::aux_io::Request;
    use files_core::folder_views::{self, Op};
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.folder_views = Default::default();
    let (tx, input) = mpsc::sync_channel(1);
    m.loaded = true;
    m.view = crate::model::View::Grid;
    m.grid_zoom = 1.4;
    m.scroll = 84;
    m.action(Action::SaveFolderView);
    assert!(m.folder_views.dispatch(&tx));
    let Request::FolderViews(id, op) = input.try_recv().unwrap() else {
        panic!()
    };
    m.load_location(f.root.join("elsewhere"));
    let result = folder_views::apply(&f.root.join("config"), &op).unwrap();
    assert_eq!(result.document.rows[0].path, f.root);
    assert!(m.folder_views.receive(id, Ok(result)));
    m.load_location(f.root.clone());
    m.loaded = true;
    m.grid_zoom = 1.;
    // Un nuevo cambio manual invalida, incluso sin otra generación.
    assert!(!m.pump_folder_restore());
    m.arm_folder_restore();
    assert!(m.pump_folder_restore());
    assert_eq!(m.grid_zoom, 1.4);
    m.action(Action::ForgetFolderView);
    assert!(m.folder_views.dispatch(&tx));
    let Request::FolderViews(id, op) = input.try_recv().unwrap() else {
        panic!()
    };
    let Op::Forget(expected) = &*op else { panic!() };
    let mut changed = expected.clone();
    changed.settings.zoom = 150;
    folder_views::apply(&f.root.join("config"), &Op::Save(changed)).unwrap();
    let result = folder_views::apply(&f.root.join("config"), &op).map_err(|e| e.to_string());
    assert!(result.is_err());
    m.folder_views.receive(id, result);
    assert!(m.folder_views.needs_resolution());
    assert!(!m.folder_views.close_ready());
}
#[test]
fn folder_view_tab_session_snapshot_has_priority_over_late_catalog() {
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.restore_tab(crate::tabs::Saved {
        path: f.root.clone(),
        history: Default::default(),
        selection: crate::refresh_selection::Snapshot::capture(
            std::iter::empty::<&std::ffi::OsStr>(),
            None,
        )
        .unwrap(),
        view: crate::model::View::List,
        sort: files_core::listing::Sort::Modified,
        descending: false,
        query: "session".into(),
        scroll: 12,
        grid_zoom: 0.8,
        pan_x: 0.,
        charge: 8192,
    });
    folder_default(m, f.root.clone());
    m.loaded = true;
    assert!(!m.pump_folder_restore());
    assert!(m.view == crate::model::View::List);
    assert!(m.sort == files_core::listing::Sort::Modified);
    assert_eq!(m.grid_zoom, 0.8);
    assert_eq!(m.scroll, 12);
    assert_eq!(m.query, "session");
}
#[test]
fn folder_view_palette_dispatches_explicit_save_without_selection_or_navigation() {
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.folder_views = Default::default();
    m.loaded = true;
    m.begin_path_edit();
    m.path_input(">Guardar vista de esta carpeta").unwrap();
    m.commit_path_edit();
    assert!(m.commands.is_some());
    m.run_command();
    assert!(m.commands.is_none());
    assert!(m.folder_views.busy());
    assert_eq!(m.path, f.root);
    assert!(m.selection.is_empty());
}
#[test]
fn folder_view_commands_preserve_search_as_default_palette_action() {
    let mut f = Fixture::new();
    f.m.loaded = true;
    f.m.action(Action::Commands);
    let state = crate::command_palette::fixture_state(&f.m);
    assert_eq!(state["matches"][0]["id"], "search");
    let ids: Vec<_> = state["matches"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| row["id"].as_str())
        .collect();
    assert!(ids.contains(&"folder-view-save") && ids.contains(&"folder-view-forget"));
}

#[test]
#[cfg(unix)]
fn allocated_size_requests_keep_visible_marks_and_focused_directory_scopes_distinct() {
    use files_core::{folder_usage::Mode, listing::Kind};
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    m.entries = vec![
        named("marked", Kind::Directory),
        named("focus", Kind::Directory),
        named("file", Kind::File),
        named("filtered", Kind::Directory),
    ];
    m.order = vec![0, 1, 2];
    m.selected = Some(1);
    m.selection = std::collections::HashSet::from([0, 2, 3]);
    let marks = m.selection.clone();
    assert!(m.request_allocated_folder_sizes(true));
    assert_eq!(m.sizes.rows(&m.path).len(), 1);
    assert_eq!(m.sizes.rows(&m.path)[0].name, "marked");
    assert_eq!(m.sizes.rows(&m.path)[0].mode, Mode::Allocated);
    let id = m.sizes.rows(&m.path)[0].id;
    assert!(!m.request_allocated_folder_sizes(true));
    assert_eq!(m.sizes.rows(&m.path)[0].id, id);
    assert_eq!((m.selection.clone(), m.selected), (marks, Some(1)));
    m.sizes.invalidate();
    m.selected = Some(2);
    assert!(!m.request_allocated_folder_sizes(false));
    assert!(m.sizes.rows(&m.path).is_empty());
    m.selected = Some(1);
    assert!(m.request_allocated_folder_sizes(false));
    assert_eq!(m.sizes.rows(&m.path)[0].name, "focus");
    assert_eq!(m.sizes.rows(&m.path)[0].mode, Mode::Allocated);
}

#[test]
#[cfg(unix)]
fn marked_sizes_capture_visible_directory_names_only_and_busy_rejection_preserves_batch() {
    use files_core::listing::Kind;
    use std::os::unix::ffi::OsStringExt;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    let raw = std::ffi::OsString::from_vec(b"raw-\xff\n".to_vec());
    m.entries = vec![
        named("A", Kind::Directory),
        named("Unmarked", Kind::Directory),
        files_core::listing::Entry {
            name: raw.clone(),
            kind: Kind::Directory,
            metadata: None,
            stat_attempted: false,
        },
        named("file", Kind::File),
        named("link", Kind::Symlink),
        named("filtered", Kind::Directory),
    ];
    m.order = vec![0, 1, 2, 3, 4];
    m.selected = Some(1);
    m.selection = std::collections::HashSet::from([0, 2, 3, 4, 5]);
    let marks = m.selection.clone();
    assert_eq!(m.marked_size_count(), 2);
    assert!(m.request_marked_folder_sizes());
    assert_eq!(
        m.sizes
            .rows(&m.path)
            .iter()
            .map(|r| r.name.clone())
            .collect::<Vec<_>>(),
        vec!["A".into(), raw]
    );
    assert_eq!(m.selection, marks);
    assert_eq!(m.selected, Some(1));
    let ids = m
        .sizes
        .rows(&m.path)
        .iter()
        .map(|r| r.id)
        .collect::<Vec<_>>();
    m.sizes.pause(true);
    m.selection = std::collections::HashSet::from([1]);
    assert!(!m.request_marked_folder_sizes());
    assert!(m.sizes.paused());
    assert_eq!(
        ids,
        m.sizes
            .rows(&m.path)
            .iter()
            .map(|r| r.id)
            .collect::<Vec<_>>()
    );
    assert!(m.error.contains("lote"));
}
#[test]
fn marked_sizes_whole_preflight_and_empty_marks_preserve_previous_observation_and_pause() {
    use files_core::listing::Kind;
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.loaded = true;
    m.sizes
        .enqueue(&m.path, [std::ffi::OsStr::new("old")].into_iter())
        .unwrap();
    m.sizes.dispatch().unwrap();
    m.sizes.pause(true);
    m.sizes.finish(Ok((5, 1)));
    let id = m.sizes.rows(&m.path)[0].id;
    m.entries = (0..129)
        .map(|i| named(&format!("folder{i}"), Kind::Directory))
        .collect();
    m.order = (0..129).collect();
    m.selection = (0..129).collect();
    m.selected = Some(0);
    assert!(!m.request_marked_folder_sizes());
    assert!(m.error.contains("128"));
    assert_eq!(m.sizes.rows(&m.path).len(), 1);
    assert_eq!(m.sizes.rows(&m.path)[0].id, id);
    assert_eq!(
        m.sizes.measured(&m.path, std::ffi::OsStr::new("old")),
        Some(5)
    );
    assert!(m.sizes.paused());
    m.selection.clear();
    assert!(!m.request_marked_folder_sizes());
    assert!(m.error.contains("foco"));
    assert_eq!(m.sizes.rows(&m.path)[0].id, id);
}

#[test]
fn canvas_wheel_preserves_magnitude_bounds_and_zero_delta() {
    fltk_omarchy::anim::set_enabled(false);
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.view = crate::model::View::List;
    m.content_height = 10_000;
    crate::wheel_scroll::wheel(m, 200, 3, false, 700);
    assert_eq!(m.scroll, 114);
    crate::wheel_scroll::wheel(m, 200, 0, false, 700);
    assert_eq!(m.scroll, 114);
    crate::wheel_scroll::wheel(m, 200, i32::MAX, false, 700);
    assert_eq!(m.scroll, 10_000 - (700 - 134));
    crate::wheel_scroll::wheel(m, 200, i32::MIN, false, 700);
    assert_eq!(m.scroll, 0);
}
#[test]
fn canvas_columns_wheel_targets_hovered_column_and_horizontal_axis() {
    fltk_omarchy::anim::set_enabled(false);
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.view = crate::model::View::Columns;
    m.columns.area = Rect::new(68, 94, 600, 472);
    m.order = (0..1000).collect();
    m.columns.columns = (0..3)
        .map(|i| crate::columns::Column {
            id: i + 1,
            watch_id: i + 1,
            path: f.root.clone(),
            data: {
                let mut data = crate::columns::Data::default();
                data.order = (0..1000).collect();
                data
            },
        })
        .collect();
    crate::wheel_scroll::wheel(m, 68 + 2 * crate::columns_view::WIDTH + 10, 3, false, 700);
    assert_eq!(m.columns.columns[2].data.scroll, 108);
    assert_eq!(m.scroll, 0);
    assert_eq!(m.columns.columns[1].data.scroll, 0);
    crate::wheel_scroll::wheel(m, 200, 3, true, 700);
    assert_eq!(m.columns.horizontal, 180);
    assert_eq!(m.columns.columns[2].data.scroll, 108);
}

#[test]
fn trash_catalog_ignores_closed_or_reloaded_requests_and_blocks_file_actions() {
    let mut f = Fixture::new();
    let m = &mut f.m;
    m.action(Action::OpenTrash);
    let first = m.trash_browser.as_ref().unwrap().id;
    let mutation = m.mutation_id;
    let old_path = m.path.clone();
    m.action(Action::Trash);
    m.action(Action::Navigate("/should-not-navigate".into()));
    assert_eq!(m.mutation_id, mutation);
    assert_eq!(m.path, old_path);
    assert!(crate::native_drop::blocked(m));
    assert!(m.text_context().is_none());
    m.action(Action::TrashBrowser(
        first,
        crate::trash_browser::Command::Reload,
    ));
    let current = m.trash_browser.as_ref().unwrap().id;
    assert_ne!(first, current);
    assert!(!m.receive_trash_catalog(first, Ok(Default::default())));
    m.action(Action::TrashBrowser(
        first,
        crate::trash_browser::Command::Close,
    ));
    assert!(m.trash_browser.is_some());
    assert!(m.receive_trash_catalog(current, Ok(Default::default())));
    m.action(Action::TrashBrowser(
        current,
        crate::trash_browser::Command::Close,
    ));
    assert!(m.trash_browser.is_none());
    assert!(!m.receive_trash_catalog(current, Ok(Default::default())));
}
