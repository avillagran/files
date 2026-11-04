//! Fixture-owned state/input contracts, without synthetic native input.
use crate::{
    aux_io::AuxIo,
    context_menu::{self, Target},
    model::{Action, Hit, Model, View},
    painter::Rect,
};
use files_core::listing::{Entry, Kind};
use std::{
    ffi::OsString,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
};
struct Fixture {
    model: Model,
    path: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = files_core::theme::home()
            .join(".cache/files-test")
            .join(format!(
                "hidden-context-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&path).unwrap();
        let mut model = Model::new(path.clone());
        let (tx, _) = mpsc::sync_channel(32);
        let (_, rx) = mpsc::channel();
        model.aux = AuxIo { tx, rx };
        model.show_hidden = false;
        model.loaded = true;
        model.generation = 42;
        model.entries = ["alpha.txt", "beta.txt", ".secret", ".folder"]
            .iter()
            .map(|name| Entry {
                name: OsString::from(name),
                kind: if *name == ".folder" {
                    Kind::Directory
                } else {
                    Kind::File
                },
                metadata: None,
                stat_attempted: false,
            })
            .collect();
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            for bytes in [b".\xff".to_vec(), b"raw\xff".to_vec()] {
                model.entries.push(Entry {
                    name: OsString::from_vec(bytes),
                    kind: Kind::File,
                    metadata: None,
                    stat_attempted: false,
                });
            }
        }
        model.rebuild();
        Self { model, path }
    }
    fn hits(&mut self) {
        self.model.hits = [(0, 100), (1, 140), (2, 180)]
            .into_iter()
            .map(|(i, y)| Hit {
                generation: 42,
                rect: Rect::new(70, y, 180, 30),
                action: Action::Entry(i),
            })
            .collect();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
#[test]
fn hiding_prunes_raw_selection_focus_anchor_and_stale_hit_operations() {
    let mut f = Fixture::new();
    let m = &mut f.model;
    assert!(!m.show_hidden);
    assert!(
        m.order
            .iter()
            .all(|&i| !files_core::visibility::is_hidden(&m.entries[i].name))
    );
    m.show_hidden = true;
    m.rebuild();
    m.selection = [0, 2, 3].into_iter().collect();
    m.selected = Some(2);
    m.anchor = Some(3);
    m.toggle_hidden();
    assert!(!m.show_hidden);
    assert!(
        m.selection
            .iter()
            .all(|&i| !files_core::visibility::is_hidden(&m.entries[i].name))
    );
    assert!(
        m.selected
            .is_none_or(|i| !files_core::visibility::is_hidden(&m.entries[i].name))
    );
    assert!(m.anchor.is_none());
    assert!(m.hits.is_empty());
    m.select_entry(2, false, false);
    assert_ne!(m.selected, Some(2));
    m.selection = [2].into_iter().collect();
    m.selected = Some(2);
    m.action(Action::Open);
    assert!(m.open_job.is_none() && m.selection.is_empty() && m.selected.is_none());
}
#[test]
fn four_views_share_visibility_and_query_cannot_reveal_hidden_names() {
    let mut f = Fixture::new();
    let m = &mut f.model;
    for view in [View::Groups, View::List, View::Spatial, View::SpaceMap] {
        m.view = view;
        m.query = "secret".into();
        m.rebuild();
        assert!(!m.order.contains(&2));
        m.show_hidden = true;
        m.rebuild();
        assert!(m.order.contains(&2));
        m.show_hidden = false;
    }
    m.query.clear();
    m.view = View::List;
    m.rebuild();
    #[cfg(unix)]
    {
        assert!(m.order.contains(&5));
        assert_eq!(m.entries[5].name.as_encoded_bytes(), b"raw\xff");
        assert!(!m.order.contains(&4));
    }
    m.mutation_applied = true;
    m.toggle_hidden();
    assert!(!m.show_hidden);
    m.mutation_applied = false;
}
#[test]
fn right_click_preserves_selected_batch_retargets_outside_and_never_opens() {
    let mut f = Fixture::new();
    f.hits();
    let m = &mut f.model;
    m.selection = [0, 1].into_iter().collect();
    m.selected = Some(0);
    assert!(context_menu::open_at(m, 80, 110, 800, 600));
    assert_eq!(m.selection.len(), 2);
    assert_eq!(m.context_menu.as_ref().unwrap().target, Target::Entries);
    let menu = m.context_menu.as_ref().unwrap();
    assert!(
        menu.items
            .iter()
            .find(|item| item.label.contains("Propiedades"))
            .expect("properties entry remains visible")
            .action
            .is_none()
    );
    assert!(
        menu.items
            .iter()
            .find(|item| item.label.contains("Mover selección"))
            .expect("move entry")
            .action
            .as_ref()
            .is_some_and(|action| matches!(action, Action::MoveFiles))
    );
    assert!(m.open_job.is_none() && m.rename_dialog.is_none() && m.copy_form.is_none());
    context_menu::dismiss(m);
    f.hits();
    f.model.selection = [0].into_iter().collect();
    assert!(context_menu::open_at(&mut f.model, 80, 150, 800, 600));
    assert_eq!(f.model.selection, [1].into_iter().collect());
    assert_eq!(f.model.selected, Some(1));
}
#[test]
fn background_is_not_the_previous_selection_and_outside_click_only_dismisses() {
    let mut f = Fixture::new();
    f.hits();
    let m = &mut f.model;
    m.selection = [0, 1].into_iter().collect();
    m.selected = Some(0);
    assert!(context_menu::open_at(m, 500, 400, 800, 600));
    let menu = m.context_menu.as_ref().unwrap();
    assert_eq!(menu.target, Target::Background);
    assert!(menu.items.iter().all(|i| !matches!(
        i.action,
        Some(
            Action::Open | Action::Trash | Action::Rename | Action::CopyFiles | Action::Properties
        )
    )));
    assert!(m.selection.is_empty() && m.selected.is_none());
    assert!(menu.rect.x + menu.rect.w <= 800 && menu.rect.y + menu.rect.h <= 600);
    context_menu::pointer(m, 2, 2, true);
    assert!(m.context_menu.is_none() && m.rename_dialog.is_none() && m.mutations_drained());
    f.hits();
    assert!(!context_menu::open_at(&mut f.model, 80, 190, 800, 600));
    assert!(!context_menu::open_at(&mut f.model, 400, 20, 800, 600));
}
#[test]
fn keyboard_context_reuses_selection_and_escape_does_not_open() {
    let mut f = Fixture::new();
    f.hits();
    let m = &mut f.model;
    m.selection = [0].into_iter().collect();
    m.selected = Some(0);
    assert!(context_menu::open_keyboard(m, 800, 440));
    assert_eq!(m.context_menu.as_ref().unwrap().target, Target::Entries);
    assert!(crate::window_chrome::blocked(m));
    // Focus starts on the first action (section titles are skipped) and
    // Down moves to the next action, never onto a title.
    let first = m.context_menu.as_ref().unwrap().focused;
    assert!(
        m.context_menu.as_ref().unwrap().items[first]
            .action
            .is_some()
    );
    assert!(m.context_menu.as_ref().unwrap().items[0].section);
    context_menu::key(m, fltk::enums::Key::Down);
    let next = m.context_menu.as_ref().unwrap().focused;
    assert!(
        next > first
            && m.context_menu.as_ref().unwrap().items[next]
                .action
                .is_some()
    );
    context_menu::key(m, fltk::enums::Key::Escape);
    assert!(m.context_menu.is_none() && m.open_job.is_none());
}
