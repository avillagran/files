//! Tree generation without a window system or D-Bus: descriptions are
//! converted to AccessKit nodes and checked field by field, and whole
//! updates are fed to AccessKit's consumer (which panics on inconsistent
//! trees, like the platform adapters).

use super::tree::{byte_index, char_index, clean_label, nodes_of, virt};
use super::{Action, Live, Node, Role};
use accesskit as ak;
use std::collections::HashMap;

fn map(n: &Node, id: u64) -> (HashMap<u64, ak::Node>, Option<u64>) {
    let (nodes, focus) = nodes_of(n, id);
    (nodes.into_iter().map(|(i, n)| (i.0, n)).collect(), focus)
}

#[test]
fn button_switch_slider_roles_names_states_actions() {
    let mut b = Node::default();
    b.role(Role::Button)
        .name("Add")
        .focusable()
        .action(Action::Click)
        .shortcut("Ctrl+N")
        .toggled(true);
    let (m, _) = map(&b, 1);
    let n = &m[&1];
    assert_eq!(n.role(), ak::Role::Button);
    assert_eq!(n.label(), Some("Add"));
    assert_eq!(n.toggled(), Some(ak::Toggled::True));
    assert_eq!(n.keyboard_shortcut(), Some("Ctrl+N"));
    assert!(n.supports_action(ak::Action::Click) && n.supports_action(ak::Action::Focus));

    let mut s = Node::default();
    s.role(Role::Slider)
        .name("Volume")
        .numeric(0.4, 0.0, 1.0, 0.05)
        .value("0.40")
        .action(Action::Increment)
        .action(Action::Decrement);
    let (m, _) = map(&s, 2);
    let n = &m[&2];
    assert_eq!(
        (
            n.numeric_value(),
            n.min_numeric_value(),
            n.max_numeric_value(),
            n.numeric_value_step()
        ),
        (Some(0.4), Some(0.0), Some(1.0), Some(0.05))
    );
    assert!(n.supports_action(ak::Action::Increment) && n.supports_action(ak::Action::Decrement));

    let mut d = Node::default();
    d.role(Role::Switch)
        .name("Wi-Fi")
        .toggled(false)
        .disabled(true)
        .focusable()
        .action(Action::Click);
    let (m, _) = map(&d, 3);
    let n = &m[&3];
    assert!(n.is_disabled());
    assert!(
        !n.supports_action(ak::Action::Click),
        "disabled controls cannot be clicked"
    );
    assert!(
        n.supports_action(ak::Action::Focus),
        "but stay focusable, so they are found"
    );
}

#[test]
fn labels_read_their_text_as_value_and_live_regions() {
    let mut l = Node::default();
    l.role(Role::Label).name("Hello Ada").live(Live::Polite);
    let (m, _) = map(&l, 5);
    assert_eq!(m[&5].value(), Some("Hello Ada"));
    assert_eq!(m[&5].label(), None);
    assert_eq!(m[&5].live(), Some(ak::Live::Polite));
}

#[test]
fn text_input_runs_caret_selection_and_password_mask() {
    let mut t = Node::default();
    // "wö" selected on the second line: characters 6..8 of "héllo\nwörld".
    t.role(Role::TextInput)
        .name("Message")
        .value("héllo\nwörld")
        .text_selection(6, 8);
    let (m, _) = map(&t, 9);
    let n = &m[&9];
    assert_eq!(n.children().len(), 2, "one text run per line");
    let r0 = &m[&n.children()[0].0];
    let r1 = &m[&n.children()[1].0];
    assert_eq!(
        (r0.role(), r0.value()),
        (ak::Role::TextRun, Some("héllo\n"))
    );
    assert_eq!(r0.character_lengths(), &[1, 2, 1, 1, 1, 1]);
    assert_eq!(r1.value(), Some("wörld"));
    let sel = n.text_selection().expect("selection");
    assert_eq!(
        (sel.anchor.node, sel.anchor.character_index),
        (n.children()[1], 0)
    );
    assert_eq!(
        (sel.focus.node, sel.focus.character_index),
        (n.children()[1], 2)
    );

    let mut p = Node::default();
    p.role(Role::PasswordInput)
        .value("secret")
        .text_selection(6, 6);
    let (m, _) = map(&p, 10);
    assert_eq!(m[&10].value(), Some("••••••"));
    let run = &m[&m[&10].children()[0].0];
    assert_eq!(run.value(), Some("••••••"), "the run is masked too");
}

#[test]
fn virtual_rows_positions_and_focused_child() {
    let mut list = Node::default();
    list.role(Role::ListBox).focusable();
    for i in 40..60u64 {
        list.child(i, |r| {
            r.role(Role::ListBoxOption)
                .name(&format!("Item {i}"))
                .position(i as usize + 1, 100_000)
                .selected(i == 42);
            if i == 42 {
                r.focused();
            }
        });
    }
    let (m, focus) = map(&list, 20);
    assert_eq!(
        m[&20].children().len(),
        20,
        "only the rows given, not the whole list"
    );
    let row = &m[&virt(20, 42)];
    assert_eq!(
        (
            row.label(),
            row.position_in_set(),
            row.size_of_set(),
            row.is_selected()
        ),
        (Some("Item 42"), Some(42), Some(100_000), Some(true))
    );
    // AccessKit reads the set size from the container.
    assert_eq!(m[&20].size_of_set(), Some(100_000));
    assert_eq!(focus, Some(virt(20, 42)));
}

#[test]
fn nested_virtual_children_and_duplicate_keys() {
    let mut grid = Node::default();
    grid.role(Role::Grid).counts(3, 2);
    for r in 0..2u64 {
        grid.child(r, |row| {
            row.role(Role::Row).row_index(r as usize);
            for c in 0..2u64 {
                row.child(c, |cell| {
                    cell.role(Role::Cell)
                        .name(&format!("{r},{c}"))
                        .column_index(c as usize);
                });
            }
            if r == 1 {
                row.child(1, |dup| {
                    dup.role(Role::Cell).name("dup");
                });
                row.focused();
            }
        });
    }
    let (m, focus) = map(&grid, 30);
    let row1 = &m[&virt(30, 1)];
    assert_eq!(
        row1.children().len(),
        2,
        "a repeated key is dropped, not sent twice"
    );
    assert_eq!(m[&virt(virt(30, 1), 1)].label(), Some("1,1"));
    assert_eq!(
        (m[&30].row_count(), m[&30].column_count()),
        (Some(3), Some(2))
    );
    assert_eq!(focus, Some(virt(30, 1)));
}

#[test]
fn consumer_accepts_full_and_incremental_updates() {
    use accesskit_consumer::Tree;
    let build = |selected: u64, extra: bool| {
        let mut root = Node::default();
        root.role(Role::Window).name("Files");
        root.child(1, |l| {
            l.role(Role::ListBox);
            for i in 0..5u64 {
                l.child(i, |r| {
                    r.role(Role::ListBoxOption)
                        .name(&format!("f{i}"))
                        .selected(i == selected);
                    if i == selected {
                        r.focused();
                    }
                });
            }
        });
        if extra {
            root.child(2, |d| {
                d.role(Role::AlertDialog).name("Delete?").modal(true);
            });
        }
        nodes_of(&root, 1)
    };
    let (nodes, focus) = build(0, false);
    let full = ak::TreeUpdate {
        nodes,
        tree: Some(ak::TreeInfo::new(ak::NodeId(1))),
        tree_id: ak::TreeId::ROOT,
        focus: ak::NodeId(focus.unwrap()),
    };
    let mut tree = Tree::new(full.clone(), true);
    // Incremental: only nodes that differ, as the sync sends them.
    let old: HashMap<ak::NodeId, ak::Node> = full.nodes.into_iter().collect();
    let (nodes, focus) = build(3, true);
    let changed: Vec<(ak::NodeId, ak::Node)> = nodes
        .into_iter()
        .filter(|(id, n)| old.get(id) != Some(n))
        .collect();
    assert!(
        changed.len() < old.len(),
        "not a full tree: {} of {}",
        changed.len(),
        old.len()
    );
    let mut handler = NoChanges;
    tree.update_and_process_changes(
        ak::TreeUpdate {
            nodes: changed,
            tree: None,
            tree_id: ak::TreeId::ROOT,
            focus: ak::NodeId(focus.unwrap()),
        },
        &mut handler,
    );
    let state = tree.state();
    assert_eq!(
        state.focus().and_then(|n| n.label()),
        Some("f3".to_string())
    );
    assert!(
        state
            .root()
            .children()
            .any(|c| c.role() == ak::Role::AlertDialog),
        "the dialog was added"
    );
}

struct NoChanges;
impl accesskit_consumer::TreeChangeHandler for NoChanges {
    fn node_added(&mut self, _: &accesskit_consumer::NodeRef) {}
    fn node_updated(&mut self, _: &accesskit_consumer::NodeRef, _: &accesskit_consumer::NodeRef) {}
    fn focus_moved(
        &mut self,
        _: Option<&accesskit_consumer::NodeRef>,
        _: Option<&accesskit_consumer::NodeRef>,
    ) {
    }
    fn node_removed(&mut self, _: &accesskit_consumer::NodeRef) {}
}

#[test]
fn fltk_labels_and_offsets() {
    assert_eq!(clean_label("&Save"), "Save");
    assert_eq!(clean_label("Fish && Chips"), "Fish & Chips");
    assert_eq!(clean_label("@->"), "");
    let s = "añb€";
    assert_eq!(char_index(s, 3), 2);
    assert_eq!(byte_index(s, 3), 4);
    assert_eq!(byte_index(s, 9), s.len());
}

#[test]
fn documents_with_explicit_runs_links_scroll_and_selection() {
    let mut d = Node::default();
    d.role(Role::Document)
        .name("Chat")
        .bounds(0, 40, 300, 200)
        .scroll(120.0, 900.0);
    d.child(3, |p| {
        p.role(Role::Paragraph)
            .name("Read the docs")
            .bounds(10, 300, 280, 20)
            .text_run(1 << 32, "Read the docs\n", Some((10, 300, 120, 20)));
        p.child(0, |l| {
            l.role(Role::Link).name("the docs").url("https://d.dev").action(Action::Click);
        });
    });
    d.text_selection_in((&[3, 1 << 32], 5), (&[3, 1 << 32], 8));
    let (m, _) = map(&d, 30);
    let doc = &m[&30];
    // Scrolled: the transform moves the content up, the own bounds stay put.
    assert_eq!(doc.scroll_y(), Some(120.0));
    assert_eq!(doc.scroll_y_max(), Some(900.0));
    assert_eq!(doc.transform().map(|t| t.as_coeffs()[5]), Some(-120.0));
    assert_eq!(doc.bounds().map(|b| b.y0), Some(160.0));
    // No automatic run on the document: the paragraph's explicit run.
    assert!(doc
        .children()
        .iter()
        .all(|c| m[&c.0].role() != ak::Role::TextRun));
    let p = virt(30, 3);
    let run = virt(p, 1 << 32);
    assert_eq!(m[&run].role(), ak::Role::TextRun);
    assert_eq!(m[&run].value(), Some("Read the docs\n"));
    assert_eq!(m[&run].character_lengths().len(), 14);
    let link = &m[&virt(p, 0)];
    assert_eq!((link.role(), link.url()), (ak::Role::Link, Some("https://d.dev")));
    let sel = doc.text_selection().expect("selection");
    assert_eq!((sel.anchor.node.0, sel.anchor.character_index), (run, 5));
    assert_eq!(sel.focus.character_index, 8);
    // The consumer accepts it and reads the document as text.
    let mut nodes: Vec<(ak::NodeId, ak::Node)> = m.into_iter().map(|(i, n)| (ak::NodeId(i), n)).collect();
    nodes.sort_by_key(|(i, _)| *i == ak::NodeId(30));
    let tree = accesskit_consumer::Tree::new(
        ak::TreeUpdate {
            nodes,
            tree: Some(ak::TreeInfo::new(ak::NodeId(30))),
            tree_id: ak::TreeId::ROOT,
            focus: ak::NodeId(30),
        },
        true,
    );
    let root = tree.state().root();
    assert!(root.supports_text_ranges());
    assert_eq!(root.document_range().text(), "Read the docs\n");
    assert_eq!(root.text_selection().map(|r| r.text()).as_deref(), Some("the"));
}
