//! Accessibility: screen readers and other assistive technologies (AT) read
//! and drive the app through [AccessKit] — AT-SPI on Linux/BSD (Orca),
//! UI Automation on Windows (NVDA, Narrator), NSAccessibility on macOS
//! (VoiceOver). FLTK has no accessibility bridge; this module is it.
//!
//! Behind the cargo feature **`a11y`**. Without it every function here is
//! an empty inline call and nothing is linked. With it, nothing is built
//! until an assistive technology asks: the tree is generated on activation
//! and then updated incrementally (only the nodes that changed), once per
//! event-loop turn, from the toolkit's own pre-flush check. No timers.
//!
//! Every Omarchy component describes itself (role, name from its visible
//! text, value, states, actions); plain fltk widgets get a generic node by
//! type (button, input, slider…). Actions from the AT (click, focus, set
//! value, increment, expand…) run the same code as the keyboard.
//!
//! What apps do:
//! - Name icon-only controls: `ActionButton` and header actions use their
//!   tooltip; otherwise `.accessible_name("Add tab")` ([`AccessibleExt`]).
//! - Custom widgets implement [`Accessible`] (or call [`describe`] /
//!   [`on_action`]) to expose a role, a value and virtual children.
//! - [`announce`] reads a short message (live region) without moving focus.
//! - [`snapshot`] prints the tree as text (tests, agents).
//!
//! Guide: docs/ACCESSIBILITY.md.
//!
//! [AccessKit]: https://accesskit.dev

use fltk::prelude::*;

#[cfg(feature = "a11y")]
pub(crate) mod imp;
// The OS adapters (AT-SPI, UIA, NSAccessibility) live in the platform layer.
#[cfg(feature = "a11y")]
use crate::platform::a11y as platform;
#[cfg(feature = "a11y")]
mod tree;

#[cfg(all(test, feature = "a11y"))]
mod tests;

/// What a node is, for the assistive technology. A subset of the WAI-ARIA
/// roles, mapped to AccessKit roles (and from there to AT-SPI, UIA and
/// NSAccessibility roles).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Role {
    /// Not exposed with a role of its own (its children are).
    #[default]
    Unknown,
    Window,
    Group,
    Toolbar,
    Button,
    CheckBox,
    RadioButton,
    RadioGroup,
    Switch,
    Slider,
    SpinButton,
    TextInput,
    PasswordInput,
    SearchInput,
    MultilineTextInput,
    /// Static text: the name is read as the text.
    Label,
    Heading,
    Paragraph,
    /// A read-only rich document (chat transcript, markdown view).
    Document,
    /// A block of code (preformatted text).
    Code,
    /// A static table of data (rows: [`Role::Row`], cells: [`Role::Cell`]
    /// or [`Role::ColumnHeader`]). Interactive tables are [`Role::Grid`].
    Table,
    Image,
    ComboBox,
    /// A list whose items can be selected (items: [`Role::ListBoxOption`]).
    ListBox,
    ListBoxOption,
    List,
    ListItem,
    /// A table with rows and cells (rows: [`Role::Row`]).
    Grid,
    Row,
    Cell,
    ColumnHeader,
    Tree,
    TreeItem,
    TabList,
    Tab,
    TabPanel,
    Menu,
    MenuBar,
    MenuItem,
    MenuItemCheckBox,
    MenuItemRadio,
    Dialog,
    AlertDialog,
    /// A message that is read as soon as it appears (notification).
    Alert,
    /// Status information (progress card, status line), read politely.
    Status,
    ProgressIndicator,
    Link,
    ScrollView,
    Canvas,
}

/// Something an assistive technology asks a node to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// The default action (press, toggle, pick, open).
    Click,
    Focus,
    /// Set the text ([`Value::Text`]) or the number ([`Value::Number`]).
    SetValue,
    Increment,
    Decrement,
    ScrollIntoView,
    Expand,
    Collapse,
    ShowContextMenu,
    /// Move the caret or select text ([`Value::Selection`]).
    SetTextSelection,
    /// Replace the selected text (or insert at the caret): [`Value::Text`].
    ReplaceSelectedText,
}

impl Action {
    #[cfg(feature = "a11y")]
    pub(crate) const ALL: [Action; 11] = [
        Action::Click,
        Action::Focus,
        Action::SetValue,
        Action::Increment,
        Action::Decrement,
        Action::ScrollIntoView,
        Action::Expand,
        Action::Collapse,
        Action::ShowContextMenu,
        Action::SetTextSelection,
        Action::ReplaceSelectedText,
    ];

    fn bit(self) -> u16 {
        1 << (self as u16)
    }
}

/// How a live region is announced when its text changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Live {
    Off,
    /// When the user is idle (status, progress, chat messages).
    Polite,
    /// Right away, interrupting (errors, critical notifications).
    Assertive,
}

/// Data that comes with an action.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Text(String),
    Number(f64),
    /// Caret/selection in characters (not bytes): `anchor == focus` is a caret.
    Selection {
        anchor: usize,
        focus: usize,
    },
    /// A selection over text runs of virtual children (a rich document):
    /// for each end, the keys from the widget's node down to the node that
    /// holds the runs ([`Node::text_run`]), and the character in the text
    /// of those runs.
    TextRange {
        anchor: (Vec<u64>, usize),
        focus: (Vec<u64>, usize),
    },
}

/// An action request from the assistive technology, delivered on the UI
/// thread to the widget's [`on_action`] handler (or [`Accessible::act`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub action: Action,
    /// Keys of the virtual children from the widget's node down to the
    /// target (empty: the widget itself). See [`Node::child`].
    pub path: Vec<u64>,
    pub value: Option<Value>,
}

impl Request {
    /// Key of the targeted virtual child (the last one of `path`).
    pub fn key(&self) -> Option<u64> {
        self.path.last().copied()
    }
}

/// The accessible description of a widget (or of one of its virtual
/// children), filled in by its describe callback. Every setter returns
/// `&mut Self` for chaining:
///
/// ```
/// use fltk_omarchy::a11y::{Node, Role, Action};
/// let mut n = Node::default();
/// n.role(Role::Slider).name("Volume").numeric(0.4, 0.0, 1.0, 0.05).action(Action::Increment);
/// n.child(7, |row| { row.role(Role::ListBoxOption).name("Speakers").selected(true); });
/// assert_eq!(n.children().len(), 1);
/// ```
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Node {
    pub(crate) role: Role,
    pub(crate) name: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) value: Option<String>,
    pub(crate) placeholder: Option<String>,
    pub(crate) numeric: Option<(f64, f64, f64, f64)>,
    pub(crate) toggled: Option<bool>,
    pub(crate) expanded: Option<bool>,
    pub(crate) has_popup: bool,
    pub(crate) selected: Option<bool>,
    pub(crate) current: bool,
    pub(crate) disabled: Option<bool>,
    pub(crate) read_only: bool,
    pub(crate) modal: bool,
    pub(crate) multiselectable: bool,
    pub(crate) hidden: bool,
    pub(crate) live: Option<Live>,
    pub(crate) level: Option<usize>,
    pub(crate) position: Option<(usize, usize)>,
    pub(crate) row_index: Option<usize>,
    pub(crate) column_index: Option<usize>,
    pub(crate) counts: Option<(usize, usize)>,
    pub(crate) sort: Option<bool>,
    pub(crate) bounds: Option<(i32, i32, i32, i32)>,
    pub(crate) actions: u16,
    pub(crate) shortcut: Option<String>,
    pub(crate) text_selection: Option<(usize, usize)>,
    pub(crate) run_selection: Option<RunSelection>,
    pub(crate) url: Option<String>,
    pub(crate) scroll: Option<(f64, f64)>,
    /// A text run (made by [`Node::text_run`]); its text is `value`.
    pub(crate) run: bool,
    pub(crate) children: Vec<Node>,
    pub(crate) key: u64,
    pub(crate) focused: bool,
    pub(crate) leaf: bool,
    pub(crate) active_descendant: Option<Vec<u64>>,
}

impl Node {
    pub fn role(&mut self, role: Role) -> &mut Self {
        self.role = role;
        self
    }

    /// The name read for the node: its visible label. Empty strings clear it.
    pub fn name(&mut self, name: &str) -> &mut Self {
        self.name = (!name.is_empty()).then(|| name.to_string());
        self
    }

    /// Extra help read after the name and role (a tooltip, a detail line).
    pub fn description(&mut self, text: &str) -> &mut Self {
        self.description = (!text.is_empty()).then(|| text.to_string());
        self
    }

    /// The current value as text (a text field's content, a dropdown's choice).
    pub fn value(&mut self, value: &str) -> &mut Self {
        self.value = Some(value.to_string());
        self
    }

    pub fn placeholder(&mut self, text: &str) -> &mut Self {
        self.placeholder = (!text.is_empty()).then(|| text.to_string());
        self
    }

    /// A number in a range (sliders, spin buttons, progress).
    pub fn numeric(&mut self, value: f64, min: f64, max: f64, step: f64) -> &mut Self {
        self.numeric = Some((value, min, max, step));
        self
    }

    /// Checked / on (check boxes, switches, toggle buttons, radio buttons).
    pub fn toggled(&mut self, on: bool) -> &mut Self {
        self.toggled = Some(on);
        self
    }

    /// Expanded or collapsed (combo boxes, tree items, disclosures).
    pub fn expanded(&mut self, open: bool) -> &mut Self {
        self.expanded = Some(open);
        self
    }

    /// Opens a menu (a submenu entry, a menu button): read as "has
    /// popup" / "submenu". Pair it with [`Node::expanded`].
    pub fn has_popup(&mut self, on: bool) -> &mut Self {
        self.has_popup = on;
        self
    }

    /// Selected or not (list options, rows, tabs). Setting it marks the
    /// node selectable.
    pub fn selected(&mut self, on: bool) -> &mut Self {
        self.selected = Some(on);
        self
    }

    /// The current item of a set (the current page, the active row).
    pub fn current(&mut self, on: bool) -> &mut Self {
        self.current = on;
        self
    }

    pub fn disabled(&mut self, on: bool) -> &mut Self {
        self.disabled = Some(on);
        self
    }

    pub fn read_only(&mut self, on: bool) -> &mut Self {
        self.read_only = on;
        self
    }

    /// A dialog that blocks the rest of the window while open.
    pub fn modal(&mut self, on: bool) -> &mut Self {
        self.modal = on;
        self
    }

    pub fn multiselectable(&mut self, on: bool) -> &mut Self {
        self.multiselectable = on;
        self
    }

    /// Present in the tree but hidden from the AT.
    pub fn hidden(&mut self, on: bool) -> &mut Self {
        self.hidden = on;
        self
    }

    /// Makes the node a live region: changes of its name or value are read.
    pub fn live(&mut self, live: Live) -> &mut Self {
        self.live = Some(live);
        self
    }

    /// Nesting level, from 1 (tree items, headings).
    pub fn level(&mut self, level: usize) -> &mut Self {
        self.level = Some(level.max(1));
        self
    }

    /// Position in its set, from 1, and the set size: "3 of 120". Virtual
    /// lists give it to the rows they expose, so the AT knows the full size.
    pub fn position(&mut self, position: usize, of: usize) -> &mut Self {
        self.position = Some((position.max(1), of));
        self
    }

    /// Row index from 0 (rows of a grid).
    pub fn row_index(&mut self, index: usize) -> &mut Self {
        self.row_index = Some(index);
        self
    }

    /// Column index from 0 (cells, column headers).
    pub fn column_index(&mut self, index: usize) -> &mut Self {
        self.column_index = Some(index);
        self
    }

    /// Row and column counts of a grid (the whole data, not only the
    /// exposed rows).
    pub fn counts(&mut self, rows: usize, columns: usize) -> &mut Self {
        self.counts = Some((rows, columns));
        self
    }

    /// Sort direction of a column header (`true` = ascending).
    pub fn sort(&mut self, ascending: bool) -> &mut Self {
        self.sort = Some(ascending);
        self
    }

    /// Rectangle in window coordinates. Widgets get theirs automatically;
    /// virtual children should set it (rows, tabs, menu entries).
    pub fn bounds(&mut self, x: i32, y: i32, w: i32, h: i32) -> &mut Self {
        self.bounds = Some((x, y, w, h));
        self
    }

    /// Adds an action the node supports (handled by [`on_action`]).
    pub fn action(&mut self, action: Action) -> &mut Self {
        self.actions |= action.bit();
        self
    }

    /// The node can take keyboard focus (adds [`Action::Focus`]).
    pub fn focusable(&mut self) -> &mut Self {
        self.action(Action::Focus)
    }

    /// Keyboard shortcut, as shown in menus ("Ctrl+S").
    pub fn shortcut(&mut self, keys: &str) -> &mut Self {
        self.shortcut = (!keys.is_empty()).then(|| keys.to_string());
        self
    }

    /// Caret and selection of a text node, in characters of its value.
    pub fn text_selection(&mut self, anchor: usize, focus: usize) -> &mut Self {
        self.text_selection = Some((anchor, focus));
        self
    }

    /// Selection over text runs of virtual children (rich documents): for
    /// each end, the keys from this node down to a run made with
    /// [`text_run`](Self::text_run), and the character in that run.
    pub fn text_selection_in(
        &mut self,
        anchor: (&[u64], usize),
        focus: (&[u64], usize),
    ) -> &mut Self {
        self.run_selection = Some(((anchor.0.to_vec(), anchor.1), (focus.0.to_vec(), focus.1)));
        self
    }

    /// The link target of a [`Role::Link`] (read by the AT, AT-SPI
    /// Hyperlink).
    pub fn url(&mut self, url: &str) -> &mut Self {
        self.url = (!url.is_empty()).then(|| url.to_string());
        self
    }

    /// A scrolled view: its children's bounds are in content coordinates
    /// (as if scrolled to the top), and the AT gets the offset `y` of
    /// `0..=max`. Scrolling then changes only this node.
    pub fn scroll(&mut self, y: f64, max: f64) -> &mut Self {
        self.scroll = Some((y, max.max(0.0)));
        self
    }

    /// Adds one line of this node's text as a text run (character, word
    /// and line navigation, caret and selection for the AT), with its
    /// window rectangle when known. Text nodes ([`Role::Document`],
    /// [`Role::Paragraph`], inputs) without explicit runs anywhere below
    /// them get one run per line of their value (or name) automatically.
    pub fn text_run(
        &mut self,
        key: u64,
        text: &str,
        bounds: Option<(i32, i32, i32, i32)>,
    ) -> &mut Self {
        self.children.push(Node {
            key,
            run: true,
            value: Some(text.to_string()),
            bounds,
            ..Node::default()
        });
        self
    }

    /// Adds a virtual child (a list row, a tab, a menu entry) identified by
    /// a stable `key` among its siblings (an item ID, an index).
    pub fn child(&mut self, key: u64, f: impl FnOnce(&mut Node)) -> &mut Self {
        let mut c = Node {
            key,
            ..Node::default()
        };
        f(&mut c);
        self.children.push(c);
        self
    }

    /// This virtual child has the keyboard focus while its widget has it
    /// (the cursor row of a list, the highlighted menu entry).
    pub fn focused(&mut self) -> &mut Self {
        self.focused = true;
        self
    }

    /// The virtual descendant (keys from this node down, see
    /// [`Node::child`]) that is active while this node keeps the keyboard:
    /// the highlighted entry of a completion list under a text input
    /// (`aria-activedescendant`).
    pub fn active_descendant(&mut self, path: &[u64]) -> &mut Self {
        self.active_descendant = (!path.is_empty()).then(|| path.to_vec());
        self
    }

    /// The widget's fltk children are an implementation detail: do not
    /// expose them (a text field's inner input).
    pub fn leaf(&mut self) -> &mut Self {
        self.leaf = true;
        self
    }

    // ---- getters (tests, custom adapters)

    pub fn get_role(&self) -> Role {
        self.role
    }

    pub fn get_name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn get_value(&self) -> Option<&str> {
        self.value.as_deref()
    }

    pub fn children(&self) -> &[Node] {
        &self.children
    }

    pub fn supports(&self, action: Action) -> bool {
        self.actions & action.bit() != 0
    }
}

/// Ends of a selection over virtual text runs: the key path from the node
/// to the run and the character in the run.
pub(crate) type RunSelection = ((Vec<u64>, usize), (Vec<u64>, usize));

/// Implemented by custom widgets that want an accessible node (a rich
/// text view, a canvas with regions). Register it with [`register`].
///
/// ```
/// use fltk::{prelude::*, widget::Widget};
/// use fltk_omarchy::a11y::{self, Accessible, Action, Node, Request, Role};
/// #[derive(Clone)]
/// struct Transcript { widget: Widget, text: std::rc::Rc<std::cell::RefCell<String>> }
/// impl Accessible for Transcript {
///     fn describe(&self, n: &mut Node) {
///         n.role(Role::Document).name("Conversation").value(&self.text.borrow()).read_only(true);
///     }
///     fn act(&self, r: &Request) -> bool {
///         r.action == Action::ScrollIntoView
///     }
/// }
/// # fn build(t: Transcript) { a11y::register(&t.widget.clone(), t); }
/// ```
pub trait Accessible {
    /// Fills in the node of this widget. Called only while an assistive
    /// technology is active, on the UI thread, when the window changed.
    fn describe(&self, node: &mut Node);

    /// Runs an action requested by the AT; return true when handled
    /// (otherwise the default runs: focus, or the widget's click).
    fn act(&self, request: &Request) -> bool {
        let _ = request;
        false
    }
}

/// Describes `widget` with `a` (see [`Accessible`]). Replaces a previous
/// description; the registration ends when the widget is deleted.
pub fn register<W: WidgetExt, A: Accessible + 'static>(widget: &W, a: A) {
    #[cfg(feature = "a11y")]
    {
        let a = std::rc::Rc::new(a);
        let b = a.clone();
        imp::set_describe(widget, std::rc::Rc::new(move |n: &mut Node| a.describe(n)));
        imp::set_action(widget, Box::new(move |r: &Request| b.act(r)));
    }
    #[cfg(not(feature = "a11y"))]
    {
        let _ = (widget, a);
    }
}

/// Sets the describe callback of `widget` (role, name, value, children).
/// It runs only while an assistive technology is active.
#[inline]
pub fn describe<W: WidgetExt>(widget: &W, f: impl Fn(&mut Node) + 'static) {
    #[cfg(feature = "a11y")]
    imp::set_describe(widget, std::rc::Rc::new(f));
    #[cfg(not(feature = "a11y"))]
    let _ = (widget, f);
}

/// Handles actions requested by the AT on `widget` or its virtual
/// children; return true when handled.
#[inline]
pub fn on_action<W: WidgetExt>(widget: &W, f: impl FnMut(&Request) -> bool + 'static) {
    #[cfg(feature = "a11y")]
    imp::set_action(widget, Box::new(f));
    #[cfg(not(feature = "a11y"))]
    let _ = (widget, f);
}

/// The name read for `widget`, overriding its visible text (icon-only
/// buttons, unlabeled sliders and switches).
#[inline]
pub fn set_name<W: WidgetExt + ?Sized>(widget: &W, name: &str) {
    #[cfg(feature = "a11y")]
    imp::set_name(widget, name);
    #[cfg(not(feature = "a11y"))]
    let _ = (widget, name);
}

/// Extra help read for `widget` after its name and role.
#[inline]
pub fn set_description<W: WidgetExt + ?Sized>(widget: &W, text: &str) {
    #[cfg(feature = "a11y")]
    imp::set_description(widget, text);
    #[cfg(not(feature = "a11y"))]
    let _ = (widget, text);
}

/// Tells the AT that `widget`'s description changed although it did not
/// redraw (data changed behind a widget that is not visible yet). Redraws
/// are noticed automatically.
#[inline]
pub fn changed<W: WidgetExt + ?Sized>(widget: &W) {
    let _ = widget;
    #[cfg(feature = "a11y")]
    imp::mark_changed();
}

/// Reads `text` without moving focus (a live region of the focused
/// window): "Saved", "3 results", "Assistant is typing". Nothing happens
/// when no AT is active.
#[inline]
pub fn announce(text: &str, live: Live) {
    #[cfg(feature = "a11y")]
    imp::announce(text, live);
    #[cfg(not(feature = "a11y"))]
    let _ = (text, live);
}

/// True while an assistive technology reads this app (some window's tree
/// was requested). Always false without the `a11y` feature.
pub fn is_active() -> bool {
    #[cfg(feature = "a11y")]
    return imp::is_active();
    #[cfg(not(feature = "a11y"))]
    false
}

/// Starts following windows for assistive technologies (idempotent).
/// [`App::new`](crate::App::new) calls it; plain fltk apps call it once
/// after `fltk::app::App::default()` and `fltk::app::lock()` (the lock lets
/// the adapter's thread wake the event loop when an AT asks). Set
/// `OMARCHY_FLTK_A11Y=off` to keep the platform adapters off.
#[inline]
pub fn install() {
    #[cfg(feature = "a11y")]
    imp::install();
}

/// The accessible tree of `window` as indented text, one node per line
/// (`role "name" = value [states]`), built now even if no AT is active.
/// None without the `a11y` feature. For tests and agents:
/// check names and roles the way `snapshot` checks pixels.
pub fn snapshot<W: WindowExt>(window: &W) -> Option<String> {
    #[cfg(feature = "a11y")]
    return Some(tree::snapshot(&imp::base(window)));
    #[cfg(not(feature = "a11y"))]
    {
        let _ = window;
        None
    }
}

/// Builder methods for every component (`Button`, `ToggleSwitch`,
/// `Slider`… anything that derefs to an fltk widget):
///
/// ```no_run
/// use fltk_omarchy::{a11y::AccessibleExt, widgets::*};
/// let add = Button::icon(glyph::PLUS, "").accessible_name("New tab");
/// ```
pub trait AccessibleExt: Sized {
    /// The name read for this control (see [`set_name`]).
    fn accessible_name(self, name: &str) -> Self;
    /// Extra help read after the name (see [`set_description`]).
    fn accessible_description(self, text: &str) -> Self;
}

impl<T> AccessibleExt for T
where
    T: std::ops::Deref,
    T::Target: WidgetExt,
{
    fn accessible_name(self, name: &str) -> Self {
        set_name(&*self, name);
        self
    }

    fn accessible_description(self, text: &str) -> Self {
        set_description(&*self, text);
        self
    }
}

// ------------------------------------------------------- toolkit internals

/// The default action of a component (what Enter/Space/click run).
#[inline]
pub(crate) fn default_click<W: WidgetExt>(widget: &W, f: std::rc::Rc<dyn Fn()>) {
    #[cfg(feature = "a11y")]
    imp::set_click(widget, f);
    #[cfg(not(feature = "a11y"))]
    let _ = (widget, f);
}

/// `layer` is the popup of `trigger` (combo boxes report it as expanded).
#[inline]
pub(crate) fn popup_of<L: WidgetExt, T: WidgetExt>(layer: &L, trigger: &T) {
    #[cfg(feature = "a11y")]
    imp::popup_of(layer, trigger);
    #[cfg(not(feature = "a11y"))]
    let _ = (layer, trigger);
}

/// True while a popup opened by `trigger` is open.
#[inline]
pub(crate) fn has_popup<T: WidgetExt>(trigger: &T) -> bool {
    #[cfg(feature = "a11y")]
    return imp::has_popup(trigger);
    #[cfg(not(feature = "a11y"))]
    {
        let _ = trigger;
        false
    }
}

/// A number as read by screen readers ("3", "0.45").
pub(crate) fn number_text(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v:.2}")
    }
}

/// Character index of an fltk byte offset (caret, mark).
pub(crate) fn char_at(s: &str, byte: i32) -> usize {
    let b = (byte.max(0) as usize).min(s.len());
    s.char_indices().take_while(|(i, _)| *i < b).count()
}

/// fltk byte offset of a character index.
pub(crate) fn byte_at(s: &str, ch: usize) -> i32 {
    s.char_indices().nth(ch).map_or(s.len(), |(i, _)| i) as i32
}

/// Window rectangle of a widget, for virtual children's bounds.
#[cfg(feature = "a11y")]
pub(crate) fn rect<W: WidgetExt>(w: &W) -> (i32, i32, i32, i32) {
    (w.x(), w.y(), w.w(), w.h())
}

/// Testing hooks (record the updates sent to a window's adapter).
#[cfg(feature = "a11y")]
#[doc(hidden)]
pub mod testing {
    pub use super::imp::testing::*;
}
