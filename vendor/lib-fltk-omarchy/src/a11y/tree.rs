//! Builds the AccessKit tree of one window from the fltk widget tree, the
//! registered descriptions and the generic fallback for plain fltk widgets.

use super::{imp, Action, Live, Node, Role};
use accesskit as ak;
use fltk::{app, prelude::*, widget::Widget};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

/// IDs of virtual nodes (rows, tabs, text runs) have the top bit set;
/// widget IDs are a counter below it.
const VIRTUAL: u64 = 1 << 63;
/// Keys of nodes the builder adds itself (text runs, announcements).
const RUN_KEY: u64 = u64::MAX - 1;
const ANNOUNCE_KEY: u64 = u64::MAX - 2;

pub(crate) fn virt(parent: u64, key: u64) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    parent.hash(&mut h);
    key.hash(&mut h);
    h.finish() | VIRTUAL
}

/// Where an action on a node goes: the widget and the virtual child path.
#[derive(Clone)]
pub(crate) struct Route {
    pub widget: Widget,
    pub path: Vec<u64>,
}

pub(crate) struct Built {
    pub root: u64,
    pub nodes: Vec<(ak::NodeId, ak::Node)>,
    pub focus: u64,
    pub routes: HashMap<u64, Route>,
}

struct Ctx {
    nodes: Vec<(ak::NodeId, ak::Node)>,
    ids: HashSet<u64>,
    routes: HashMap<u64, Route>,
    by_ptr: HashMap<usize, u64>,
    focus_in: HashMap<u64, u64>,
}

fn ptr<W: WidgetExt>(w: &W) -> usize {
    w.as_widget_ptr() as usize
}

/// The whole tree of `win` (a top-level window).
pub(crate) fn build(win: &Widget) -> Built {
    let mut cx = Ctx {
        nodes: Vec::new(),
        ids: HashSet::new(),
        routes: HashMap::new(),
        by_ptr: HashMap::new(),
        focus_in: HashMap::new(),
    };
    let info = imp::info(win);
    let root = info.id;
    cx.ids.insert(root);
    cx.by_ptr.insert(ptr(win), root);
    cx.routes.insert(
        root,
        Route {
            widget: win.clone(),
            path: Vec::new(),
        },
    );
    let mut n = Node::default();
    n.role(Role::Window)
        .name(&clean_label(&win.label()))
        .bounds(0, 0, win.w(), win.h());
    if let Some(d) = &info.describe {
        d(&mut n);
    }
    apply_overrides(&mut n, &info, win);
    let mut kids = Vec::new();
    if let Some(g) = win.as_group() {
        for i in 0..g.children() {
            if let Some(c) = g.child(i) {
                kids.extend(visit(&c, (0, 0), &mut cx));
            }
        }
    }
    if let Some((text, live)) = imp::announcement(root) {
        let id = virt(root, ANNOUNCE_KEY);
        if cx.ids.insert(id) {
            let mut a = ak::Node::new(ak::Role::Status);
            a.set_label(text);
            a.set_live(live_of(live));
            a.set_bounds(ak::Rect::new(0.0, 0.0, 1.0, 1.0));
            cx.nodes.push((ak::NodeId(id), a));
            kids.push(id);
        }
    }
    let scale = imp::scale_of(win);
    let mut a = convert(&n, root, kids, &mut cx);
    if (scale - 1.0).abs() > 0.01 {
        a.set_transform(ak::Affine::scale(scale));
    }
    cx.nodes.push((ak::NodeId(root), a));
    let focus = focus_of(&cx).unwrap_or(root);
    Built {
        root,
        nodes: cx.nodes,
        focus,
        routes: cx.routes,
    }
}

/// The node of the focused widget (or its focused virtual child).
fn focus_of(cx: &Ctx) -> Option<u64> {
    let f = app::focus()?;
    // SAFETY: fltk returned a live widget pointer.
    let mut w = Some(unsafe { Widget::from_widget_ptr(f.as_widget_ptr()) });
    while let Some(cur) = w {
        if let Some(&id) = cx.by_ptr.get(&ptr(&cur)) {
            return Some(cx.focus_in.get(&id).copied().unwrap_or(id));
        }
        w = cur.parent().map(|p| imp::base(&p));
    }
    None
}

/// Node IDs `w` contributes to its parent: its own, or its children's when
/// it is only layout (an unnamed Flex).
fn visit(w: &Widget, off: (i32, i32), cx: &mut Ctx) -> Vec<u64> {
    if !w.visible() {
        return Vec::new();
    }
    let info = imp::info(w);
    let mut n = Node {
        bounds: Some((off.0 + w.x(), off.1 + w.y(), w.w(), w.h())),
        ..Node::default()
    };
    if let Some(d) = &info.describe {
        d(&mut n);
    } else if let Some((panels, active)) = crate::focus::hosted_panels(w) {
        describe_panels(w, &panels, active, &mut n);
    } else {
        generic(w, &mut n);
    }
    apply_overrides(&mut n, &info, w);
    let child_off = if w.as_window().is_some() {
        (off.0 + w.x(), off.1 + w.y())
    } else {
        off
    };
    let group = if n.leaf { None } else { w.as_group() };
    if n.role == Role::Unknown {
        let mut kids = Vec::new();
        if let Some(g) = group {
            for i in 0..g.children() {
                if let Some(c) = g.child(i) {
                    kids.extend(visit(&c, child_off, cx));
                }
            }
        }
        return kids;
    }
    let id = info.id;
    if !cx.ids.insert(id) {
        return Vec::new();
    }
    cx.by_ptr.insert(ptr(w), id);
    cx.routes.insert(
        id,
        Route {
            widget: w.clone(),
            path: Vec::new(),
        },
    );
    let mut kids = Vec::new();
    let mut focus = None;
    for c in &n.children {
        if let Some(cid) = emit_virtual(c, id, Some(w), vec![c.key], cx, &mut focus) {
            kids.push(cid);
        }
    }
    if let Some(f) = focus {
        cx.focus_in.insert(id, f);
    }
    if let Some(g) = group {
        for i in 0..g.children() {
            if let Some(c) = g.child(i) {
                kids.extend(visit(&c, child_off, cx));
            }
        }
    }
    let a = convert(&n, id, kids, cx);
    cx.nodes.push((ak::NodeId(id), a));
    vec![id]
}

fn emit_virtual(
    n: &Node,
    parent: u64,
    w: Option<&Widget>,
    path: Vec<u64>,
    cx: &mut Ctx,
    focus: &mut Option<u64>,
) -> Option<u64> {
    let id = virt(parent, n.key);
    if !cx.ids.insert(id) {
        return None;
    }
    if let Some(w) = w {
        cx.routes.insert(
            id,
            Route {
                widget: w.clone(),
                path: path.clone(),
            },
        );
    }
    let mut kids = Vec::new();
    let mut inner = None;
    for c in &n.children {
        let mut p = path.clone();
        p.push(c.key);
        if let Some(cid) = emit_virtual(c, id, w, p, cx, &mut inner) {
            kids.push(cid);
        }
    }
    if focus.is_none() {
        *focus = inner.or(n.focused.then_some(id));
    }
    let a = convert(n, id, kids, cx);
    cx.nodes.push((ak::NodeId(id), a));
    Some(id)
}

fn apply_overrides(n: &mut Node, info: &imp::Info, w: &Widget) {
    if let Some(name) = &info.name {
        n.name = Some(name.clone());
    }
    if let Some(d) = &info.description {
        n.description = Some(d.clone());
    }
    let tip = w.tooltip().unwrap_or_default();
    if !tip.is_empty() && n.role != Role::Unknown {
        if n.name.is_none() {
            n.name = Some(tip);
        } else if n.description.is_none() && n.name.as_deref() != Some(tip.as_str()) {
            n.description = Some(tip);
        }
    }
    if !w.active_r() && n.disabled.is_none() && n.role != Role::Window {
        n.disabled = Some(true);
    }
}

fn is_text(role: Role) -> bool {
    matches!(
        role,
        Role::TextInput
            | Role::PasswordInput
            | Role::SearchInput
            | Role::MultilineTextInput
            | Role::Document
            | Role::Paragraph
    )
}

fn role_of(r: Role) -> ak::Role {
    use ak::Role as A;
    match r {
        Role::Unknown => A::GenericContainer,
        Role::Window => A::Window,
        Role::Group => A::Group,
        Role::Toolbar => A::Toolbar,
        Role::Button => A::Button,
        Role::CheckBox => A::CheckBox,
        Role::RadioButton => A::RadioButton,
        Role::RadioGroup => A::RadioGroup,
        Role::Switch => A::Switch,
        Role::Slider => A::Slider,
        Role::SpinButton => A::SpinButton,
        Role::TextInput => A::TextInput,
        Role::PasswordInput => A::PasswordInput,
        Role::SearchInput => A::SearchInput,
        Role::MultilineTextInput => A::MultilineTextInput,
        Role::Label => A::Label,
        Role::Heading => A::Heading,
        Role::Paragraph => A::Paragraph,
        Role::Document => A::Document,
        Role::Code => A::Code,
        Role::Table => A::Table,
        Role::Image => A::Image,
        Role::ComboBox => A::ComboBox,
        Role::ListBox => A::ListBox,
        Role::ListBoxOption => A::ListBoxOption,
        Role::List => A::List,
        Role::ListItem => A::ListItem,
        Role::Grid => A::Grid,
        Role::Row => A::Row,
        Role::Cell => A::Cell,
        Role::ColumnHeader => A::ColumnHeader,
        Role::Tree => A::Tree,
        Role::TreeItem => A::TreeItem,
        Role::TabList => A::TabList,
        Role::Tab => A::Tab,
        Role::TabPanel => A::TabPanel,
        Role::Menu => A::Menu,
        Role::MenuBar => A::MenuBar,
        Role::MenuItem => A::MenuItem,
        Role::MenuItemCheckBox => A::MenuItemCheckBox,
        Role::MenuItemRadio => A::MenuItemRadio,
        Role::Dialog => A::Dialog,
        Role::AlertDialog => A::AlertDialog,
        Role::Alert => A::Alert,
        Role::Status => A::Status,
        Role::ProgressIndicator => A::ProgressIndicator,
        Role::Link => A::Link,
        Role::ScrollView => A::ScrollView,
        Role::Canvas => A::Canvas,
    }
}

fn action_of(a: Action) -> ak::Action {
    match a {
        Action::Click => ak::Action::Click,
        Action::Focus => ak::Action::Focus,
        Action::SetValue => ak::Action::SetValue,
        Action::Increment => ak::Action::Increment,
        Action::Decrement => ak::Action::Decrement,
        Action::ScrollIntoView => ak::Action::ScrollIntoView,
        Action::Expand => ak::Action::Expand,
        Action::Collapse => ak::Action::Collapse,
        Action::ShowContextMenu => ak::Action::ShowContextMenu,
        Action::SetTextSelection => ak::Action::SetTextSelection,
        Action::ReplaceSelectedText => ak::Action::ReplaceSelectedText,
    }
}

pub(crate) fn action_from(a: ak::Action) -> Option<Action> {
    Action::ALL.into_iter().find(|x| action_of(*x) == a)
}

fn live_of(l: Live) -> ak::Live {
    match l {
        Live::Off => ak::Live::Off,
        Live::Polite => ak::Live::Polite,
        Live::Assertive => ak::Live::Assertive,
    }
}

/// True when `n` or a virtual child below it is an explicit text run.
fn has_runs(n: &Node) -> bool {
    n.children.iter().any(|c| c.run || has_runs(c))
}

/// One text run: its characters and word starts for the AT.
fn run_node(text: &str, bounds: Option<(i32, i32, i32, i32)>) -> ak::Node {
    let mut r = ak::Node::new(ak::Role::TextRun);
    r.set_value(text);
    let lens: Vec<u8> = text.chars().map(|c| c.len_utf8() as u8).collect();
    let mut words = Vec::new();
    let mut prev_space = true;
    for (ci, c) in text.chars().enumerate() {
        let space = c.is_whitespace();
        if prev_space && !space && ci <= u8::MAX as usize {
            words.push(ci as u8);
        }
        prev_space = space;
    }
    let n = lens.len();
    r.set_character_lengths(lens);
    if !words.is_empty() && n <= u8::MAX as usize {
        r.set_word_starts(words);
    }
    if let Some((x, y, w, h)) = bounds {
        r.set_bounds(ak::Rect::new(
            x as f64,
            y as f64,
            (x + w.max(0)) as f64,
            (y + h.max(0)) as f64,
        ));
    }
    r
}

fn convert(n: &Node, id: u64, mut kids: Vec<u64>, cx: &mut Ctx) -> ak::Node {
    if n.run {
        return run_node(n.value.as_deref().unwrap_or(""), n.bounds);
    }
    let mut a = ak::Node::new(role_of(n.role));
    if let Some(name) = &n.name {
        if n.role == Role::Label {
            a.set_value(name.as_str());
        } else {
            a.set_label(name.as_str());
        }
    }
    if let Some(d) = &n.description {
        a.set_description(d.as_str());
    }
    let shown_value = n.value.as_ref().map(|v| {
        if n.role == Role::PasswordInput {
            "•".repeat(v.chars().count())
        } else {
            v.clone()
        }
    });
    if let Some(v) = &shown_value {
        if n.role != Role::Label || n.name.is_none() {
            a.set_value(v.as_str());
        }
    }
    if let Some(p) = &n.placeholder {
        a.set_placeholder(p.as_str());
    }
    if let Some((v, min, max, step)) = n.numeric {
        a.set_numeric_value(v);
        a.set_min_numeric_value(min);
        a.set_max_numeric_value(max);
        if step > 0.0 {
            a.set_numeric_value_step(step);
        }
    }
    if let Some(t) = n.toggled {
        a.set_toggled(if t {
            ak::Toggled::True
        } else {
            ak::Toggled::False
        });
    }
    if let Some(e) = n.expanded {
        a.set_expanded(e);
    }
    if n.has_popup {
        a.set_has_popup(ak::HasPopup::Menu);
    }
    if let Some(s) = n.selected {
        a.set_selected(s);
    }
    if n.current {
        a.set_aria_current(ak::AriaCurrent::True);
    }
    if n.disabled == Some(true) {
        a.set_disabled();
    }
    if n.read_only {
        a.set_read_only();
    }
    if n.modal {
        a.set_modal();
    }
    if n.multiselectable {
        a.set_multiselectable();
    }
    if n.hidden {
        a.set_hidden();
    }
    if let Some(l) = n.live {
        a.set_live(live_of(l));
    }
    if let Some(l) = n.level {
        a.set_level(l);
    }
    if let Some((p, of)) = n.position {
        // AccessKit: zero-based, and the set size belongs to the container
        // (kept on the item too for `snapshot`).
        a.set_position_in_set(p - 1);
        a.set_size_of_set(of);
    }
    if let Some(r) = n.row_index {
        a.set_row_index(r);
    }
    if let Some(c) = n.column_index {
        a.set_column_index(c);
    }
    if n.position.is_none() {
        if let Some((_, of)) = n.children.iter().find_map(|c| c.position) {
            a.set_size_of_set(of);
        }
    }
    if let Some((r, c)) = n.counts {
        a.set_row_count(r);
        a.set_column_count(c);
    }
    if let Some(asc) = n.sort {
        a.set_sort_direction(if asc {
            ak::SortDirection::Ascending
        } else {
            ak::SortDirection::Descending
        });
    }
    if let Some((x, y, w, h)) = n.bounds {
        // A scrolled node's own transform applies to its bounds too.
        let dy = n.scroll.map_or(0.0, |(y, _)| y);
        a.set_bounds(ak::Rect::new(
            x as f64,
            y as f64 + dy,
            (x + w.max(0)) as f64,
            (y + h.max(0)) as f64 + dy,
        ));
    }
    if let Some((y, max)) = n.scroll {
        a.set_transform(ak::Affine::translate((0.0, -y)));
        a.set_scroll_y(y);
        a.set_scroll_y_min(0.0);
        a.set_scroll_y_max(max);
    }
    if let Some(u) = &n.url {
        a.set_url(u.as_str());
    }
    if let Some((anchor, focus)) = &n.run_selection {
        let pos = |(path, c): &(Vec<u64>, usize)| ak::TextPosition {
            node: ak::NodeId(path.iter().fold(id, |p, k| virt(p, *k))),
            character_index: *c,
        };
        a.set_text_selection(ak::TextSelection {
            anchor: pos(anchor),
            focus: pos(focus),
        });
    }
    if let Some(path) = &n.active_descendant {
        a.set_active_descendant(ak::NodeId(path.iter().fold(id, |p, k| virt(p, *k))));
    }
    if let Some(k) = &n.shortcut {
        a.set_keyboard_shortcut(k.as_str());
    }
    for act in Action::ALL {
        if n.supports(act) && n.disabled != Some(true) {
            a.add_action(action_of(act));
        }
    }
    if n.disabled == Some(true) && n.supports(Action::Focus) {
        a.add_action(ak::Action::Focus);
    }
    // Text: one run per line, so the AT gets characters, words and the caret.
    if is_text(n.role) && !has_runs(n) {
        let text = shown_value
            .clone()
            .or_else(|| {
                if n.role == Role::Paragraph {
                    n.name.clone()
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let lines: Vec<&str> = if text.is_empty() {
            vec![""]
        } else {
            text.split_inclusive('\n').collect()
        };
        let mut starts = Vec::with_capacity(lines.len());
        let mut at = 0usize;
        let mut runs = Vec::with_capacity(lines.len());
        for (i, line) in lines.iter().enumerate() {
            let rid = virt(id, RUN_KEY - i as u64);
            if !cx.ids.insert(rid) {
                continue;
            }
            let r = run_node(line, n.bounds);
            starts.push((rid, at, line.chars().count()));
            at += line.chars().count();
            runs.push((rid, r));
        }
        if let Some((anchor, focus)) = n.text_selection {
            let pos = |c: usize| -> Option<ak::TextPosition> {
                let c = c.min(at);
                let (rid, start, len) = starts
                    .iter()
                    .copied()
                    .find(|(_, s, l)| c >= *s && c < s + (*l).max(1))
                    .or_else(|| starts.last().copied())?;
                Some(ak::TextPosition {
                    node: ak::NodeId(rid),
                    character_index: (c - start).min(len),
                })
            };
            if let (Some(a0), Some(f0)) = (pos(anchor), pos(focus)) {
                a.set_text_selection(ak::TextSelection {
                    anchor: a0,
                    focus: f0,
                });
            }
        }
        for (rid, r) in runs {
            kids.push(rid);
            cx.nodes.push((ak::NodeId(rid), r));
        }
    }
    a.set_children(kids.into_iter().map(ak::NodeId).collect::<Vec<_>>());
    a
}

/// The AccessKit nodes of a description and its virtual children, and the
/// focused descendant (unit tests: no widget, no window).
#[cfg(test)]
pub(crate) fn nodes_of(n: &Node, id: u64) -> (Vec<(ak::NodeId, ak::Node)>, Option<u64>) {
    let mut cx = Ctx {
        nodes: Vec::new(),
        ids: HashSet::from([id]),
        routes: HashMap::new(),
        by_ptr: HashMap::new(),
        focus_in: HashMap::new(),
    };
    let mut kids = Vec::new();
    let mut focus = None;
    for c in &n.children {
        if let Some(cid) = emit_virtual(c, id, None, vec![c.key], &mut cx, &mut focus) {
            kids.push(cid);
        }
    }
    let a = convert(n, id, kids, &mut cx);
    cx.nodes.push((ak::NodeId(id), a));
    (cx.nodes, focus)
}

/// FLTK labels: `&` marks a shortcut letter, `@` starts a symbol.
pub(crate) fn clean_label(s: &str) -> String {
    if s.starts_with('@') {
        return String::new();
    }
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '&' {
            if it.peek() == Some(&'&') {
                out.push('&');
                it.next();
            }
            continue;
        }
        out.push(c);
    }
    out
}

// ---------------------------------------------------------------- generic

/// Node for a plain fltk widget, by its C++ type.
fn generic(w: &Widget, n: &mut Node) {
    use fltk::{button, frame, input, menu, misc, text, valuator};
    let p = w.as_widget_ptr();
    let label = clean_label(&w.label());
    let focusable = w.has_visible_focus();
    // Order matters: dynamic casts accept subclasses.
    if let Some(b) = button::CheckButton::from_dyn_widget_ptr(p as _) {
        n.role(Role::CheckBox)
            .name(&label)
            .toggled(b.value())
            .action(Action::Click);
    } else if let Some(b) = button::RoundButton::from_dyn_widget_ptr(p as _) {
        n.role(Role::RadioButton)
            .name(&label)
            .toggled(b.value())
            .action(Action::Click);
    } else if let Some(b) = button::LightButton::from_dyn_widget_ptr(p as _) {
        n.role(Role::Switch)
            .name(&label)
            .toggled(b.value())
            .action(Action::Click);
    } else if let Some(b) = button::Button::from_dyn_widget_ptr(p as _) {
        let kind = b.get_type::<button::ButtonType>();
        n.role(if kind == button::ButtonType::Radio {
            Role::RadioButton
        } else {
            Role::Button
        })
        .name(&label)
        .action(Action::Click);
        if matches!(kind, button::ButtonType::Toggle | button::ButtonType::Radio) {
            n.toggled(b.value());
        }
    } else if let Some(i) = input::Input::from_dyn_widget_ptr(p as _) {
        let kind = i.get_type::<input::InputType>() as i32;
        let role = if kind & 7 == 5 {
            Role::PasswordInput
        } else if kind & 7 == 4 {
            Role::MultilineTextInput
        } else {
            Role::TextInput
        };
        n.role(role)
            .name(&label)
            .value(&i.value())
            .action(Action::SetValue)
            .action(Action::SetTextSelection);
        if kind & 8 != 0 || i.readonly() {
            n.read_only(true);
        }
        n.text_selection(
            char_index(&i.value(), i.mark() as usize),
            char_index(&i.value(), i.position() as usize),
        );
    } else if let Some(t) = text::TextDisplay::from_dyn_widget_ptr(p as _) {
        let editable = text::TextEditor::from_dyn_widget_ptr(p as _).is_some();
        let value = t.buffer().map(|b| b.text()).unwrap_or_default();
        n.role(if editable {
            Role::MultilineTextInput
        } else {
            Role::Document
        })
        .name(&label)
        .value(&value)
        .read_only(!editable);
        let pos = char_index(&value, t.insert_position().max(0) as usize);
        n.text_selection(pos, pos);
    } else if let Some(s) = misc::Spinner::from_dyn_widget_ptr(p as _) {
        n.role(Role::SpinButton)
            .name(&label)
            .numeric(s.value(), s.minimum(), s.maximum(), s.step())
            .value(&fmt_num(s.value()))
            .action(Action::Increment)
            .action(Action::Decrement)
            .action(Action::SetValue)
            .leaf();
    } else if let Some(s) = valuator::Scrollbar::from_dyn_widget_ptr(p as _) {
        let _ = s; // scroll bars belong to their scroll view
    } else if let Some(s) = valuator::Slider::from_dyn_widget_ptr(p as _) {
        valuator_node(
            n,
            &label,
            s.value(),
            s.minimum(),
            s.maximum(),
            s.step(),
            Role::Slider,
        );
    } else if let Some(s) = valuator::Dial::from_dyn_widget_ptr(p as _) {
        valuator_node(
            n,
            &label,
            s.value(),
            s.minimum(),
            s.maximum(),
            s.step(),
            Role::Slider,
        );
    } else if let Some(s) = valuator::Roller::from_dyn_widget_ptr(p as _) {
        valuator_node(
            n,
            &label,
            s.value(),
            s.minimum(),
            s.maximum(),
            s.step(),
            Role::Slider,
        );
    } else if let Some(s) = valuator::Counter::from_dyn_widget_ptr(p as _) {
        valuator_node(
            n,
            &label,
            s.value(),
            s.minimum(),
            s.maximum(),
            s.step(),
            Role::SpinButton,
        );
    } else if let Some(s) = valuator::ValueInput::from_dyn_widget_ptr(p as _) {
        valuator_node(
            n,
            &label,
            s.value(),
            s.minimum(),
            s.maximum(),
            s.step(),
            Role::SpinButton,
        );
    } else if let Some(pr) = misc::Progress::from_dyn_widget_ptr(p as _) {
        n.role(Role::ProgressIndicator).name(&label).numeric(
            pr.value(),
            pr.minimum(),
            pr.maximum(),
            0.0,
        );
    } else if let Some(c) = menu::Choice::from_dyn_widget_ptr(p as _) {
        n.role(Role::ComboBox)
            .name(&label)
            .value(&c.choice().unwrap_or_default())
            .expanded(false);
    } else if menu::MenuButton::from_dyn_widget_ptr(p as _).is_some() {
        n.role(Role::Button).name(&label).action(Action::Click);
    } else if menu::MenuBar::from_dyn_widget_ptr(p as _).is_some() {
        n.role(Role::MenuBar).name(&label);
    } else if let Some(b) = fltk::browser::Browser::from_dyn_widget_ptr(p as _) {
        n.role(Role::ListBox).name(&label);
        let size = b.size().max(0) as usize;
        let sel = b.value().max(0) as usize; // 1-based, 0 = none
        let first = sel.saturating_sub(100).max(1);
        for line in first..=(first + 200).min(size) {
            let text = clean_label(
                b.text(line as i32)
                    .unwrap_or_default()
                    .trim_start_matches(['@', '.']),
            );
            n.child(line as u64, |c| {
                c.role(Role::ListBoxOption)
                    .name(&text)
                    .selected(b.selected(line as i32))
                    .position(line, size);
                if line == sel {
                    c.focused();
                }
            });
        }
    } else if frame::Frame::from_dyn_widget_ptr(p as _).is_some() {
        if !label.is_empty() {
            n.role(Role::Label).name(&label);
        }
    } else if w.as_group().is_some() {
        if !label.is_empty() && w.as_window().is_none() {
            n.role(Role::Group).name(&label);
        }
    } else if !label.is_empty() {
        n.role(Role::Label).name(&label);
    }
    if focusable && n.role != Role::Unknown && n.role != Role::Label && n.role != Role::Group {
        n.focusable();
    }
}

fn valuator_node(n: &mut Node, label: &str, v: f64, min: f64, max: f64, step: f64, role: Role) {
    let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
    n.role(role)
        .name(label)
        .numeric(v, lo, hi, step)
        .value(&fmt_num(v))
        .action(Action::Increment)
        .action(Action::Decrement)
        .action(Action::SetValue);
}

pub(crate) fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v:.2}")
    }
}

/// Character index of a byte offset.
pub(crate) fn char_index(s: &str, byte: usize) -> usize {
    let b = byte.min(s.len());
    s.char_indices().take_while(|(i, _)| *i < b).count()
}

/// Byte offset of a character index.
pub(crate) fn byte_index(s: &str, ch: usize) -> usize {
    s.char_indices().nth(ch).map_or(s.len(), |(i, _)| i)
}

// --------------------------------------------------------- keyboard panels

/// Items exposed around the cursor of a panel that has no viewport info.
const PANEL_WINDOW: usize = 25;

/// A canvas with keyboard panels (`focus::host`): one list box per panel,
/// the items near the cursor, the cursor item focused.
fn describe_panels(
    w: &Widget,
    panels: &[crate::panel::KeyboardPanel],
    active: usize,
    n: &mut Node,
) {
    let label = clean_label(&w.label());
    n.role(Role::Group).name(&label).focusable();
    let (x, y, ww, hh) = super::rect(w);
    for (pi, p) in panels.iter().enumerate() {
        let len = p.len();
        let cursor = p.cursor_index();
        let focused = pi == active;
        n.child(pi as u64 + 1, |list| {
            list.role(Role::ListBox)
                .name(&p.name())
                .multiselectable(true)
                .bounds(x, y, ww, hh)
                .focusable();
            let c = cursor.unwrap_or(0);
            let (from, to) = (
                c.saturating_sub(PANEL_WINDOW),
                (c + PANEL_WINDOW + 1).min(len),
            );
            for i in from..to {
                let Some(id) = p.id_at(i) else { continue };
                let name = p
                    .item_label(id)
                    .unwrap_or_else(|| format!("Item {}", i + 1));
                list.child(id, |item| {
                    item.role(Role::ListBoxOption)
                        .name(&name)
                        .selected(p.is_selected(id))
                        .position(i + 1, len)
                        .action(Action::Click)
                        .action(Action::ScrollIntoView)
                        .focusable();
                    if focused && cursor == Some(i) {
                        item.focused();
                    }
                });
            }
            if focused && cursor.is_none() {
                list.focused();
            }
        });
    }
}

/// Default actions on a panel host's virtual nodes (path = [panel + 1, item]).
pub(crate) fn panel_action(w: &Widget, r: &super::Request) -> bool {
    let Some((panels, _)) = crate::focus::hosted_panels(w) else {
        return false;
    };
    let Some(p) = r
        .path
        .first()
        .and_then(|k| panels.get((*k as usize).wrapping_sub(1)))
        .cloned()
    else {
        return false;
    };
    match (r.action, r.path.get(1).copied()) {
        (Action::Focus, None) => p.focus(),
        (Action::Click, Some(id)) => p.click(id, false, false),
        (Action::Focus | Action::ScrollIntoView, Some(id)) => {
            p.focus();
            p.set_cursor(Some(id));
        }
        _ => return false,
    }
    true
}

// ---------------------------------------------------------------- snapshot

/// Indented text form of the tree (tests, agents).
pub(crate) fn snapshot(win: &Widget) -> String {
    let b = build(win);
    let focus = b.focus;
    let map: HashMap<u64, ak::Node> = b.nodes.into_iter().map(|(id, n)| (id.0, n)).collect();
    let mut out = String::new();
    fn line(map: &HashMap<u64, ak::Node>, id: u64, depth: usize, focus: u64, out: &mut String) {
        let Some(n) = map.get(&id) else { return };
        if n.role() == ak::Role::TextRun {
            return;
        }
        let mut s = format!("{}{:?}", "  ".repeat(depth), n.role());
        if let Some(l) = n.label() {
            s.push_str(&format!(" {l:?}"));
        }
        if let Some(v) = n.value() {
            s.push_str(&format!(" = {v:?}"));
        } else if let Some(v) = n.numeric_value() {
            s.push_str(&format!(" = {}", fmt_num(v)));
        }
        let mut st: Vec<String> = Vec::new();
        if id == focus {
            st.push("focused".into());
        }
        match n.toggled() {
            Some(ak::Toggled::True) => st.push("checked".into()),
            Some(ak::Toggled::False) => st.push("unchecked".into()),
            _ => {}
        }
        if n.has_popup().is_some() {
            st.push("has popup".into());
        }
        if let Some(e) = n.is_expanded() {
            st.push(if e { "expanded" } else { "collapsed" }.into());
        }
        if n.is_selected() == Some(true) {
            st.push("selected".into());
        }
        if n.is_disabled() {
            st.push("disabled".into());
        }
        if n.is_modal() {
            st.push("modal".into());
        }
        if let (Some(p), Some(of)) = (n.position_in_set(), n.size_of_set()) {
            st.push(format!("{} of {of}", p + 1));
        }
        if let (Some(r), Some(c)) = (n.row_count(), n.column_count()) {
            st.push(format!("{r}x{c}"));
        }
        if let Some(l) = n.level() {
            st.push(format!("level {l}"));
        }
        if let Some(d) = n.description() {
            st.push(format!("desc {d:?}"));
        }
        if n.live().is_some_and(|l| l != ak::Live::Off) {
            st.push("live".into());
        }
        if !st.is_empty() {
            s.push_str(&format!(" [{}]", st.join(", ")));
        }
        out.push_str(&s);
        out.push('\n');
        for c in n.children() {
            line(map, c.0, depth + 1, focus, out);
        }
    }
    line(&map, b.root, 0, focus, &mut out);
    out
}
