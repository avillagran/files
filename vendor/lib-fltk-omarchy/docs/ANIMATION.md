# Animation

When the system's Animations setting is on, everything in the toolkit
moves the way Omarchy and o3 move: controls change state with Omarchy's
short transitions, layout changes slide instead of jumping, a sidebar
expands when the pointer rests on it, popups fade in and out, the tab
selection slides, list rows make room, arrive and leave, tree branches
open and close. Apps get it without writing animation code. When the
setting is off, everything jumps to its final state.

Animations run on one shared fltk timer at 60 fps, only while something
moves: an idle window costs no CPU.

## On or off

`anim::enabled()` answers, in this order:

1. The app: `anim::set_enabled(true)` / `set_enabled(false)`;
   `set_enabled(None)` follows the system again.
2. The user: `OMARCHY_FLTK_ANIM=on|off` (also `1|0`, `true|false`;
   the older `OMARCHY_FLTK_ANIMATIONS=0` still works).
3. The system setting, read live (below).
4. Nothing answered: on.

`anim::source()` says which one decided (`App`, `Env`, `System`,
`Default`); `anim::system()` is the system value (`None` when unknown).

| OS | Setting | How it is read | When it is read again |
|---|---|---|---|
| Linux, Hyprland (Omarchy) | `animations:enabled` (the Omarchy control panel's Animations switch, `omarchy toggle animations`) | `j/getoption animations:enabled` on Hyprland's socket (`{"bool": …}`, older `{"int": …}`) | Hyprland's event socket: `configreloaded` and `activewindowv2` (`hyprctl keyword` sends no event; coming back to the app after changing it does) |
| Linux, GNOME, Unity, Budgie, Pantheon, Cinnamon, MATE | `org.gnome.desktop.interface enable-animations` | `gsettings get` | when an animation starts, at most every 2 s |
| macOS | Reduce motion | `NSWorkspace.accessibilityDisplayShouldReduceMotion` | when an animation starts, at most every 2 s |
| Windows | Animation effects | `SystemParametersInfo(SPI_GETCLIENTAREAANIMATION)` | when an animation starts, at most every 2 s |
| Other Unixes, other Linux desktops | none | — | — |

One thread (`omarchy-motion`) reads the setting. It blocks while idle (no
polling): on Hyprland a second thread blocks on the event socket. The
first value is read when `App::new` starts (it waits at most 50 ms for
it). All OS code is in `src/platform/`; every OS has a fallback.

The Omarchy shell turns its own animations off the same way (its
`Style.reduceMotion` follows `animations:enabled` and re-reads it on
`configreloaded`).

## What moves, and for how long

Durations and curves come from Omarchy's shell (`/usr/share/omarchy/shell`)
and o3 (`ui/qml`). They are in `anim::ms`.

| What | Duration and curve | Taken from |
|---|---|---|
| Button color | 120 ms | Omarchy `Ui/Button.qml` |
| ActionButton, rows | 60 ms | Omarchy `PanelActionButton.qml` |
| Toggle | 100 ms | Omarchy `Ui/Toggle.qml` |
| ToggleSwitch color and knob | 120 ms OutCubic | Omarchy `Ui/ToggleSwitch.qml` |
| Slider | 140 ms OutCubic | Omarchy `Ui/PanelSlider.qml` |
| **Layout**: `fixed` sizes, children shown, hidden, added (any toolkit row or column) | 190 ms OutCubic (`ms::LAYOUT`) | o3 `PinnedKeyboardPanel.qml` `Behavior on x/y/width/height` |
| A child appearing: grows from its start edge, fades in | 190 ms; opacity `min(1, 1.8·progress)` | o3 `T3Reveal.qml` (the child keeps its size, clipped; spring opacity) |
| **Sidebar** width (push) or panel (overlay) | 190 ms OutCubic | as layout |
| Sidebar hover intent / leave grace | 350 ms / 140 ms (`ms::HOVER_OPEN`, `ms::HOVER_CLOSE`) | o3 `T3Composer.qml` quick menu (350 ms), `T3ThreadRow.qml` preview `closeDelay` (140 ms) |
| **Popups** (PopupMenu, Dropdown, SearchableDropdown, Menu, ConfirmDialog, PopupCard, ShortcutHelp, toasts, completion lists): in / out | 180 ms / 120 ms, Omarchy `Style.easeOut` (cubic Bézier 0.23, 1, 0.32, 1); the scrim fades with the card | Omarchy `Ui/PopupCard.qml` |
| **TabBar** selection (fill and accent line) slides | 160 ms OutCubic (`ms::INDICATOR`) | o3 `T3Panel.qml` group chips (160 ms OutCubic), Omarchy clock panel (160 ms OutCubic) |
| **List** rows (`List::from_model`) making room | 442 ms OutCubic (`ms::ROW_DISPLACED`) | o3 `T3Panel.qml` `RowDisplaced` (0.85 × 520 ms) |
| A row moved elsewhere (`move_key`) flies there, lifted | 520 ms InOutCubic, shadow and accent rim peaking mid-way | o3 `T3RowFlight.qml`, `rowMoveMs` |
| A row arriving | 360 ms: waits a fifth, then fades in (OutCubic) and settles (OutBack 1.2) | o3 `T3ThreadRow.qml` `arrive` |
| A row leaving | 300 ms InCubic: fades out, drifts 22 px | o3 `T3ThreadRow.qml` `depart` |
| **Tree** branch opening / closing: rows below slide | 200 ms (`ms::REVEAL`) | Omarchy `Ui/KeyboardPanel.qml` `drawnHeight` |
| **RichText**: a new message (`append_markdown`, `append_blocks`) fades in | 360 ms OutCubic; streaming into it (`push_markdown`) costs nothing | o3 row arrival |
| Wheel scrolling | 120 ms to the exact destination (unchanged, `scroll`) | — |

## Layout motion

Every toolkit container animates its own changes: `layout::row()`,
`column()`, `section()`, any `Flex` you `fit` into or `close_fitted`, and
every `Flex` of an `AppWindow` when you call `win.end()` (rows and
columns, built with plain fltk-rs too). Nothing to call:

```rust
row.fixed(&sidebar, 240);   // was 56: slides to 240 px, the content makes room
row.layout();

button.hide();               // the others close the gap
column.layout();

layout::set_visible(&panel, false); // collapses to nothing and fades out
layout::set_visible(&panel, true);  // grows back in
```

How it works: such a `Flex` draws through the toolkit. Before drawing it
lays out what changed (FLTK's lazy layout), compares its children with
what it drew last time and, if the container kept its place, moves them
there over 190 ms. Each frame is a real FLTK layout state: children get
interpolated rectangles, and when anything interrupts (a window resize, a
new change, the app calling `layout()`, animations turned off) the
children get their target sizes back and FLTK lays out again, so `fixed`
sizes are never lost. A change in the middle of a motion starts from what
is on screen.

Not animated:

- the first layout of a container;
- a container that moved or changed size (window resizes by the user, a
  parent's own motion: nested containers follow their parent at once);
- direction flips (a right-to-left language), a new theme, a language
  switch;
- a `Flex` with its own draw callback (the toolkit leaves it alone).

Hiding: `w.hide()` + `layout()` makes the others close the gap, but `w`
itself is gone at once (FLTK no longer draws hidden widgets).
`layout::set_visible(&w, false)` takes a picture of `w` first and
collapses and fades that picture while the gap closes. FLTK's state is
final at once in both cases (hidden is hidden, focus leaves it).

Opt out for a container and everything inside it:

```rust
row.animated(false);                 // FlexExt
layout::set_animated(&*win, false);  // a whole window
```

`layout::animating()` is true while any container moves (tests).

## Sidebar

`widgets::Sidebar` (also `layout::Sidebar`) is a navigation rail that
expands on hover, with o3's hover intent: resting the pointer 350 ms
expands it; it collapses 140 ms after the pointer leaves, unless focus is
inside, a menu is open over it, or it is pinned. Keyboard: Tab focuses it
and it expands at once; Up/Down/Home/End move, Enter or Space picks,
Esc collapses (focus stays). `SidebarMode::Push` (default) changes its
width in its row (the layout motion above); `SidebarMode::Overlay` keeps
the rail's width and slides the expanded panel over the content. Right to
left it is on the right. Screen readers get a tab list named "Sidebar"
with its expanded state. See [COMPONENTS.md](COMPONENTS.md#sidebar).

## Your own widgets

```rust
use fltk_omarchy::anim::{self, Anim, Ease, Tween};

// A value that follows a target (call every draw):
let mut glow: Anim<f64> = Anim::new();
let a = glow.get(if hot { 1.0 } else { 0.0 }, anim::ms::INDICATOR, Ease::OutCubic, &widget);

// A one-shot progress:
let t = Tween::start(anim::ms::POPUP_IN, Ease::EaseOut);
let k = t.value(); // 0..1, eased
if !t.done() { anim::request_redraw(&widget); }
```

`Anim::get` jumps when animations are off, on the first draw and after a
theme change. `Ease` has Omarchy's and o3's curves: `Linear`, `OutCubic`,
`InCubic`, `InOutCubic`, `InOutQuart`, `OutQuint`, `OutBack(overshoot)`,
`EaseOut` (Omarchy `Style.easeOut`).

## Tests and screenshots

`anim::set_enabled(false)` gives final states at once (`tests/ui.rs` does
it first; `docs_shots` too). To look at intermediate frames, freeze the
animation clock:

```rust
anim::set_enabled(true);
anim::clock::freeze();
sidebar.expand();
win.snapshot("t0.png")?;                       // the old layout
anim::clock::advance(Duration::from_millis(60));
win.snapshot("t60.png")?;                      // in between
anim::clock::advance(Duration::from_millis(200));
win.snapshot("end.png")?;                      // = the layout without animations
anim::clock::resume();
```

A motion starts at the first draw after the change. Popups are deleted
by a timer after their fade, also with a frozen clock.

## Cost

Measured 2026-11-04 on Asahi Linux (Apple M2), Hyprland 0.56.2, release
build, a real 900×560 window on the hidden workspace
`special:omarchy-fltk-bench` (`script/motion-bench`): a sidebar beside a
100 000-row list, hover through the toolkit's own hover path (no
synthetic OS input), 10 expand/collapse cycles, medians
([motion-bench-2026-11-04.json](motion-bench-2026-11-04.json)):

| Case | Frames | Frame interval avg / max | CPU while moving | Content paint per frame |
|---|---|---|---|---|
| Expand 44 → 240 px, on | 13 | 16.9 ms / 19.7 ms (60 fps) | 15.2 ms in 226 ms (6.7 % of a core, ≈ 1.2 ms per frame) | 0.65 ms |
| Collapse 240 → 44 px, on | 13 | 16.9 ms / 18.8 ms | 17.0 ms in 227 ms (7.5 %) | 0.62 ms |
| Expand, off | 1 | — | 1.8 ms | 0.82 ms |
| Idle before / after | — | — | 0.72 / 0.93 ms per 5 s (the theme watcher's wake-ups) | — |

The system setting came from Hyprland (`source: System`). After the
motion no frame is pending (`anim::pending() == 0`): the timer stops.

## Limitations

- Fades are drawn as a veil of the color under the widget (FLTK has no
  layer opacity): over a busy background a fading popup or row goes
  toward that color, not toward what is behind it.
- Layout motion covers the toolkit's `Flex` containers; plain `Group`,
  `Pack`, `Grid` and a `Flex` with its own draw callback jump as before.
  `w.hide()` closes the gap smoothly but `w` disappears at once; use
  `layout::set_visible` for the collapse.
- `List` row motion needs `List::from_model` (stable IDs); `List::new`
  with a count jumps. Rows that move out of view leave at once.
- GNOME, macOS and Windows have no change notification here: a change of
  the setting is picked up when the next animation starts (that one still
  follows the old value).
- Popup cards fade; they do not scale (Omarchy's 0.97 → 1 grow is left
  out).
