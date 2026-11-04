# Keyboard first (0.0.10)

Every lib-fltk-omarchy app can be used without a mouse, without writing any
keyboard code. `App::new` installs one keyboard router (`focus::install`)
and every component follows the same rules.

## Rules

1. **Tab / Shift+Tab** go through every control of the window in layout
   order and wrap around. The order is the widget tree, depth first, so it
   follows your columns and rows.
2. **Arrows act inside the focused control** (list cursor, slider, tabs,
   button group). They never jump to another control. fltk's per-group
   arrow navigation is off. An arrow the control does not use goes to the
   arrow scopes around it (see below), then to window shortcuts.
3. **Enter or Space** activate the focused control (button, toggle, row,
   dropdown trigger); Enter opens the list's cursor row.
4. **Menus and dialogs** take the keyboard when they open. Tab does not
   leave them, Esc closes them, and focus goes back to the control that had
   it. App shortcuts still work while a menu is open (see
   [Popups and app shortcuts](#popups-and-app-shortcuts)).
5. **Esc never closes an app window.**
6. **Window-wide keys** keep working wherever focus is:
   `AppWindow::shortcut("F10", ..)`, `Button::shortcut("Ctrl+S")`, and
   Ctrl+Tab / Ctrl+PageUp / Ctrl+PageDown for a `TabBar`.
7. **The focus ring is drawn only while the keyboard is in use**, like
   `:focus-visible` on the web; a mouse click hides it again. The ring is
   the theme's `[controls] focus` state. When the theme's focus looks the
   same as its hover cursor (the Omarchy default), the ring gets a
   full-strength accent border so it cannot be missed.
   `theme::set_focus_ring(FocusRing::Theme)` uses the theme tokens exactly.

## Popups and app shortcuts

A list popup (PopupMenu, Dropdown, SearchableDropdown, Menu, ShortcutHelp)
uses only its own keys. **Any other key closes the popup and then goes on
to the app**, like a press on a closed menu:

1. The popup closes. A `PopupMenu` reports `Dismiss::Closed` to
   `on_dismiss`; nothing is picked.
2. Focus goes back to the control that had it.
3. The same key goes on through fltk's normal path: the focused control,
   its parents, then `AppWindow::shortcut` / `Button::shortcut`. It runs
   once.

So F1, F10, `Ctrl+,`, `Ctrl+Q` or `Alt+1` work whether or not a menu is
open. The app needs no extra code.

| Key | In an open popup |
|---|---|
| Up / Down, PageUp / PageDown, Home / End, Enter | the popup's own keys (no Ctrl / Alt / Super) |
| `j` / `k` | navigate only with **no modifier at all**. `Ctrl+j`, `Alt+k`, `Shift+j` are not navigation |
| Esc | closes with `Dismiss::Escape`; no app shortcut sees it |
| Tab / Shift+Tab | stay in the popup (the Tab trap) |
| printable text, no Ctrl / Alt / Super | filters in Menu and SearchableDropdown; Backspace, Ctrl+U, Ctrl+Backspace edit the filter there |
| F1 in ShortcutHelp | closes the help (it is the help key) |
| anything else (F-keys, Ctrl / Alt / Super + key, letters in PopupMenu / Dropdown) | closes the popup (PopupMenu: `Dismiss::Closed`), focus back, the key reaches the app |

Two kinds of layer keep their own rules:
- **ConfirmDialog** is a decision. It keeps the keyboard until it is
  answered or cancelled: F10 or Ctrl+Q do nothing while it is open.
- **PopupCard** holds your own controls. Keys they do not use go on to the
  app and the card stays open; Esc closes it.

If a shortcut needs a menu to stay open, handle that key in the app and
reopen the menu.

## Keyboard panels (one canvas, several Tab stops)

A canvas that draws several regions (a dual pane, a sidebar and a grid)
registers them as keyboard panels: `focus::host(&canvas, &[&left, &right])`.

- Each panel is a Tab stop, in that order. The router moves between them
  inside the same widget; the canvas's own handler never sees Tab.
- While a panel has focus it sees every key first, with any modifiers.
  Keys it does not use go on as usual (the canvas handler, then window
  shortcuts), once.
- A menu or dialog opened from a panel gives focus back to that panel.
- Calling `focus::host` again replaces the list, e.g. when F9 turns the dual
  pane back into one list.

Panel keys and the API: [COMPONENTS.md](COMPONENTS.md#keyboardpanel-regions-of-one-canvas).

### Panel focus events

The app learns which panel has focus from an event, not by polling. The
toolkit's own Tab router moves focus; the app does not need one.

```rust
left.on_focus(|focused| { /* this pane gained (true) or lost (false) focus */ });
let listener = focus::on_panel_focus(|e: &focus::PanelFocus| {
    // e.previous, e.current: Option<KeyboardPanel> (None = an ordinary
    // control, a menu, or nothing); e.cause: why it changed.
});
focus::remove_panel_focus(listener);
```

`focus::focused_panel()` is the panel that has the keyboard now. The event
fires each time it changes, whatever the cause:

| `cause` | When |
|---|---|
| `Tab` / `ShiftTab` | Tab, Shift+Tab, `focus::step` |
| `Focus` | `KeyboardPanel::focus()`, `focus::first` |
| `HostChanged` | `focus::host` replaced the panel list and the focused pane went away |
| `Overlay` | a menu, dropdown or dialog opened and took the keyboard |
| `Restored` | it closed (Esc, a pick, a click outside, or a key passed on to the app) and gave focus back |
| `Other` | a click, `take_focus` in app code, a toast reached with Tab, the host hidden or deleted |

**Order guarantees**
1. Synchronous. The event runs inside the key press, click or call that
   moved focus, before it returns. When `press("Tab")`, `right.focus()` or
   `focus::host(..)` returns, the listeners have already run.
2. Before the next event. A change made while handling an event is reported
   before fltk dispatches the next one, so the next key always meets an
   updated model. A change made outside any event (app code in a timer) is
   reported at the next loop turn, before anything is painted.
3. Before the repaint. Both hosts are marked for redraw, and the paint
   happens after the listeners. A draw callback sees the new state, and so
   does the model the listeners updated.
4. Consistent. Inside a listener, `focus::focused_panel() == e.current`
   and `e.current.has_focus()`.
5. One at a time. Callbacks run in this order: the losing panel's
   `on_focus(false)`, the gaining panel's `on_focus(true)`, then the
   `on_panel_focus` listeners in registration order. If a listener moves
   focus itself, that change is reported next, as its own event with its
   own cause, never nested. Each event's `previous` is the last event's
   `current`.
6. Only real changes. Moving the cursor inside a pane, or replacing the host
   list while the active pane stays, sends nothing.

**Lifetime**
- `previous` / `current` are `KeyboardPanel` handles: shared, cheap to
  clone, and compared by identity (`==`). They stay valid after the host
  widget is deleted.
- If the host is deleted while one of its panes has focus, one event
  reports `previous: Some(pane)`, `current: None`.
- Listeners live until `remove_panel_focus`. `on_focus` lives as long as
  the panel.
- A listener may add or remove listeners, focus panels, and open or close
  menus.
- A listener that keeps a panel handle keeps that panel alive; that is
  safe.

When focus is in an ordinary control (a path field), `current` is `None`.
`focus::active_panel_of(&host)` still returns the pane that stays current
for the host (the last focused one), e.g. for the source of a file
operation started from a menu.

## Keys per component

| Component | Keys |
|---|---|
| Button, ActionButton, Row, Toggle, ToggleSwitch | Enter / Space |
| ButtonGroup | one Tab stop; Left / Right change the selection |
| TextField | typing; Enter submits |
| NumberField | Up / Down |
| Slider | Left / Right / Up / Down |
| TabBar | anywhere: Ctrl+Tab, Ctrl+Shift+Tab, Ctrl+PageDown, Ctrl+PageUp. Focused: Left / Right / Home / End, Delete closes |
| List | Up / Down, j / k, PageUp / PageDown, Home / End, Enter |
| Dropdown | Enter / Space / Down open; Up / Down, j / k (no modifier); Enter; Esc |
| SearchableDropdown | Enter / Down open; type to filter; Up / Down; Enter; Esc |
| Menu | type to filter; Up / Down, PageUp / PageDown; Enter / Right; Backspace, Ctrl+Backspace, Ctrl+U; Esc clears, then closes |
| ConfirmDialog | Left / Right / Tab; Enter; Esc cancels |
| ShortcutHelp | Up / Down / PageUp / PageDown scroll; Esc / F1 close |
| Notification | reached with Tab; Enter / Space / Delete / Esc dismiss |
| OperationToast | reached with Tab or `toast.focus()`; Left / Right choose; Enter / Space press; Esc leaves |
| PopupMenu | Up / Down, j / k with no modifier (skip disabled, wrap), PageUp / PageDown, Home / End; Enter; Esc. Other keys close it (`Closed`) and reach the app |
| PopupCard | Tab / Shift+Tab stay inside; Esc closes, focus comes back |
| Table | the KeyboardPanel keys; Enter activates |
| Tree | the KeyboardPanel keys, plus Right (expand / first child) and Left (collapse / parent) |
| ShortcutHelp | Up / Down, PageUp / PageDown, Home / End (bounded); Esc / F1 close |
| KeyboardPanel | Up / Down (Left / Right in a grid), with Ctrl (cursor only), Shift (range), Ctrl+Shift (add range); PageUp / PageDown, Home / End; Ctrl+Space, Ctrl+A; Enter |

## For app code

```rust
use fltk_omarchy::focus;

// Arrow keys for your own group of controls (panels, a sidebar):
focus::scope(&sidebar_and_content, |key| match key {
    fltk::enums::Key::Right => { /* focus the content */ true }
    fltk::enums::Key::Left => { /* focus the sidebar */ true }
    _ => false,
});

focus::first(&*win);          // focus the first control (e.g. after building)
focus::step(&*win, true);     // same as Tab (false = Shift+Tab)
focus::is_focused(&*widget);  // who has focus
focus::show_ring(&*widget);   // draw a focus ring in a custom widget?
```

A custom widget joins the Tab order when it is visible, active, has
`set_visible_focus()` and returns `true` for `Event::Focus` in its
`handle`. Draw its ring with `theme.controls.focus` when
`focus::show_ring(w)` is true.

## Testing without a keyboard

`focus::click_at(&*win, x, y)` sends a left click the same way.
`focus::press("Tab")`, `press("Shift+Tab")`, `press("Down")`,
`press("Ctrl+Tab")`, `press("Enter")` or `press("a")` send a key through the
same path as a real key press. It also works on windows that are never
shown. `tests/ui.rs` drives a whole window with it ("keyboard only" check),
and `examples/docs_shots.rs` uses it for the keyboard pictures.

## Real keyboard through the compositor (0.0.5)

`script/os-input-check` opens `examples/os_input_check` floating on the
current workspace and gives it focus. It types real keys with `wtype` (the
Wayland virtual-keyboard protocol, delivered by Hyprland like hardware
keys), then gives focus back to the window that had it. It needs the
user's go-ahead: for about 3 s, keystrokes go to the test window.

Result on 2026-10-28 (`docs/os-input-2026-10-28.json`), Hyprland 0.56.2:

| Keys | Result |
|---|---|
| `omarchy` typed into the focused field | text `"omarchy"` |
| Tab, Enter, Space | Add pressed twice |
| Tab, Right, Right | ButtonGroup on Month (2) |
| Tab, Ctrl+Tab | TabBar on Work (1), app-wide shortcut |
| Tab, Down ×3 | List cursor on Item 2 |
| F10, Down, Enter | window shortcut opened the PopupMenu, Copy (1) picked, focus back on the list |
| Shift+Tab | TabBar |

Focus trace: field → Add → ButtonGroup → TabBar → List → menu → List →
TabBar → none. The last step is the test giving focus back to the user's
window.

## Real mouse (0.0.7)

Validated twice with the user's go-ahead:
- `script/os-mouse-check` ran `ydotoold` for the duration of the test only.
  Two button clicks and a switch click registered. A header drag moved
  the window with the native compositor gesture (+80, +40).
- The user then tested by hand with `examples/playground`, every action
  logged to `build/playground.log`. The log shows the native move, the
  native resize from the corners (960×730 down to 462×402), Normal /
  Floating / Fullscreen modes, menus, lists, the table, the tree and the
  toasts, with no panic.

These tests found three bugs, now fixed:
- **The first real click on a button crashed the app** (a re-entrant
  `RefCell` borrow when the button took focus). A regression test clicks
  every clickable control through fltk's event path.
- **Partial redraws erased controls.** Hovering a control made it
  disappear, and hovering a menu hid the window behind it. Group draws now
  skip cfltk's pre-draw, and the window redraws everything that overlaps
  the damaged area in stacking order (`examples/redraw_check.rs` checks
  real window pixels).
- **A window opened floating in Hyprland could not be tiled or resized.**
  It was mapped with a fixed size to make it float. It now maps normally
  and is floated right away (`examples/mode_check.rs`).

Pointer warps over IPC (`hl.dsp.cursor.move`) were not reliable for
aiming on this two-monitor, mixed-scale setup; the real-mouse evidence
comes from the hand test.
