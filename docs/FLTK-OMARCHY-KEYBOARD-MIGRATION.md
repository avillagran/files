# Keyboard migration to lib-fltk-omarchy v0.0.2

Review of tag `v0.0.2` / `210e0d64` on 2026-10-28. The dependency and the
theme adapter are updated; the `focus::install()` router is **not yet
enabled** in Files. No focus ring or keyboard-first conformance is claimed.

Files draws Groups, List, Grid, Columns, the rail, tabs and both panes in
a single `Frame`; hits are Painter records, not FLTK widgets with independent
focus. Its existing handler delivers Tab to the dual pane, Ctrl+Tab to the
tab history and arrows to selection/grid/Columns. The library's router
offers arrows to `focus::scope`, but Tab walks real widgets;
a single canvas does not represent the dual pane's two stops. Enabling it without
migrating the canvas, or removing Tab now, would lose validated navigation. The migration
needs focusable controls/panes from the library with a virtual cursor and
selection by ID, not a second router copied into Files. Requested in
[issue #3](https://github.com/avillagran/lib-fltk-omarchy/issues/3#issuecomment-6477303133).

- Ctrl+Tab/Ctrl+Shift+Tab are existing tab shortcuts; the router lets
  modifiers through. The integration must avoid a second TabBar processing the
  same shortcut and ensure the order of overlays, omnibar and tab close.
- Ctrl+, and F10 open separate menus; F1 help. B3 and Shift+F10/Menu keep
  the pane's selection and target. They stay as they were until there is a PopupMenu
  with an observable close and viewport limits.
- v0.0.2 returns focus on close and keeps Tab inside overlays. However,
  `PopupMenu::size` still computes the full height without scroll or clamp and
  `open/open_at` return `()`, without a dismiss handle/callback. Per the
  user's instruction it is not adopted yet: [issue #1](https://github.com/avillagran/lib-fltk-omarchy/issues/1#issuecomment-6477297872),
  [issue #4](https://github.com/avillagran/lib-fltk-omarchy/issues/4#issuecomment-6477300247).
- Omnibar, B3, global actions, view navigation and the grid still use
  their existing handlers; no new focus or Tab handler was introduced.
  The TOML validation before reload stays until issue #5 is resolved. The native
  libdecor bridge is not replaced with Hyprland IPC.

Future acceptance: separate real controls, Tab/Shift+Tab order offered
by the library, a ring visible only with the keyboard, pane scope, tests
with `focus::press` in addition to Files scenes, and OS evidence if available.
Without OS input, those tests do not prove a physical keyboard or the compositor.

## Review v0.0.7 — 2026-10-28

Tag `3f5978a` adds `focus::host`/KeyboardPanel for several stops in
one canvas, and a PopupHandle with an observable close and a scrolling viewport. Those
API blockers no longer apply; the integration in Files and the equivalence of
selection by ID, tabs, omnibar, actions and close still have to be implemented and
tested. The router is still not enabled just by changing the pin. The strict `Theme::try_load`
and `reload` allow removing the local validation in a separate step.
W02/libdecor stays active until a real gesture test in Files. GPU stays
off. Do not claim OS input or a ring in Files based on the toolkit's fixtures.

`Theme::try_load`/`reload`/`last_error` now replace the duplicated local
validation. The Files watcher still handles events and the `FILES_THEME` override;
the library decides whether a reload of the Omarchy theme is valid.

### PopupMenu: integration and shortcuts

The application uses PopupMenu for Preferences, Actions and the context menu; the earlier
drawing remains only for old smoke scenes that still need it. The
internal scene `toolkit-menu-ui-1793228594559034966.json` checks clamp,
scroll, the four close reasons, F1, F10, Ctrl+, and Ctrl+j. The first failure
with v0.0.8 (`toolkit-menu-ui-1793228420725853726.json`) was caused by the fixture's own
isolation: it blocked delivery to the canvas. The exception is only
enabled during the synthetic call. The historical v0.0.7 failure remains
in `toolkit-menu-ui-1793227807327186226.json`; issue #6 was closed by the
library. These scenes do not prove OS input. W02 stays active and resources and
startup are still NOT_MEASURED.

### Pin v0.0.8 — 2026-10-28

Tag v0.0.8 pins `7d20e19e94133654f7453682d04a4099036a9c4b`,
with a single copy of fltk/fltk-sys 1.5.23 and GPU disabled. The header
guard stays active; the evidence is in `fltk-omarchy-v008-headers.json`.
The pin by itself does not enable the experimental PopupMenu adapter: step
(c) requires verifying the shortcuts and their integration before adopting it.

### KeyboardPanel: pending focus notification

In v0.0.8 `focus::host` keeps the active region internally; `set_active`
repaints, but it does not notify Files of a change caused by Tab. Files needs
to switch the active model synchronously so that the path, toolbar
and operations belong to the focused pane. Querying `focused_panel()`
later does not guarantee that order against the next event. A notification
API was requested in [issue #7](https://github.com/avillagran/lib-fltk-omarchy/issues/7).
This is a static review; it does not prove OS input. Until it is resolved the
existing behavior is kept, without enabling a second Tab handler.

### Pin v0.0.9 — 2026-10-28

Tag v0.0.9 pins `c2ab990c8293bcf93c6a69fba016b57b05430c33` and brings
pane focus notifications (issue #7 closed). The migration of Files to
KeyboardPanel still has to be wired up and validated; the pin change does not enable
the router by itself. The header guard and the lock are recorded in
`fltk-omarchy-v009-headers.json`; GPU stays disabled.

### Surfaces on theme reload

Files takes the active Omarchy theme by default; the Tokyo Night option is an
explicit local alternative. Painter surfaces now derive from the same
validated Theme as the palette, so colors from the previous theme are not kept after
a reload. The library does not yet expose `darker_background`,
`lighter_background` and `bright_foreground` from colors.toml; existing
popup/control tokens are used while [issue #8](https://github.com/avillagran/lib-fltk-omarchy/issues/8)
requests those exact values. It does not imply exact equivalence with every theme.
