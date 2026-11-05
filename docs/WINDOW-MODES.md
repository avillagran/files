# Window and menus (0.0.4)

Explicit user priority, 2026-10-26. It replaces the forced borderless Linux
window: **Normal** is now the default preference and uses a standard window
with FLTK/libdecor decorations, managed by the compositor. Floating, panel, overlay,
always-on-top or a global rule are not requested. A compositor with a
tiled layout may decide that moving/resizing that window does not apply.

Preferences (Nerd Font gear button, Ctrl+,) and Actions (Nerd Font bolt button,
F10) are separate, mutually exclusive menus. Their semantic names and tooltips are
«Preferencias» and «Acciones». The canvas has a label with both shortcuts; this
does not prove a native AT-SPI/screen reader tree, which is pending under accessibility.
Actions only contains open/preview/copy/rename/create/undo/redo.
Preferences contains window mode, interface corners and theme; the views
are still reachable in the header and with Ctrl+1/2/3. Esc closes either menu.

Modes supported by FLTK:

| config.toml key | Label | Requested behavior |
|---|---|---|
| normal | Normal | Decorated, not maximized nor fullscreen; the compositor decides placement |
| fullscreen | Fullscreen | Native fullscreen; keeps Preferences to get out |

Only an explicit choice changes `window_mode`; resizing/moving from
the compositor does not rewrite that choice. Mode and `corners` are saved together in
a single atomic write on the existing worker, keeping unknown fields/tables.
ACK per revision, retry after an error and drain on close. Changes
made while a write is pending coalesce into the latest preferences snapshot.
The theme stays independent, as does the visual corner factor.

## Header and grips

Normal has four inner 24×24 px targets and a minimum window of 700×440.
Header controls and path chips/editing always win the hit test.
The rest of its first 58 px, when free, requests a move; file content
is never a window zone. NW/SE and NE/SW diagonals for the grips; the Wayland cursor theme's
`grab`/`grabbing` if they exist, falling back to FLTK's Hand/Move. Overlays,
menus, dialogs, a spatial selection being dragged or closing block these gestures.

The own C++ bridge uses the real structures of the **pinned FLTK 1.5.23/API 1.4.5,
ABI 1.4.0**; it does not guess offsets or modify the registry. It checks the Wayland backend,
its own shown window, a left FL_PUSH, belowmouse of that window, and the current
pointer/seat/serial focus before `libdecor_frame_move/resize`. It does not keep
or fabricate serials. It clears pushed/state like the FLTK driver after the request;
the native configure events apply the resize and the opposite-corner anchoring. There is no
geometry loop and no Hyprland fallback. The compositor coalesces the configure events, without
carrying over the reference panel's 50 ms QML timer.

The cursors check their own focus before touching the seat. A successful return
means the request was sent, **not** that the compositor accepted it. On other
backends the custom gesture reports the capability as unavailable; the standard modes and decorations
still go through FLTK. Native move/resize outside Linux is pending.

## Building the adapter

`cc` (MIT/Apache), already in the lockfile, becomes a direct build-dependency. The identical
fltk-sys is also a build-dependency only to order header generation;
no new backend/runtime. FLTK does not export Cargo include/source metadata:
`build.rs` locates its pinned-version source already downloaded by Cargo and its
headers generated in this profile; it rejects ambiguous sources or configurations
that differ in config.h **or** FL/fl_config.h (only the two strings
FLTK_DATADIR/FLTK_DOCDIR are excluded, as they change with the host/target artifact path). Overrides for vendor/special
environments: `FILES_FLTK_SOURCE_DIR` (cfltk/fltk root) and `FILES_FLTK_CONFIG_DIR`
(generated root with both headers). It does not download or edit dependencies.
This private coupling must be reviewed when updating FLTK; compatibility with an
arbitrary installed FLTK or a validated Linux cross-build is not claimed.

## Evidence

`scripts/window-ui-check.py` uses internal targets/handlers and the real framebuffer;
it reads hyprctl clients to observe modes and never uses it to move/float/resize.
It checks the default, persistence across processes, unknown fields, menu
exclusion, control/header/four-corner zones and normal close after ACK.
The fixture does not call the gesture bridge: real OS input/move/resize/cursors
are still pending because the CUA controller sees no windows. Resources/startup NOT_MEASURED
because of the freeze; the main PSS footer of captures is not extrapolated to the tree/budget.

The first scene failed waiting for Maximized: Omarchy's real rule
`default/hypr/windows.lua` suppresses maximize events for every app.
The FLTK adapter set maximize_active, but hyprctl still reported fullscreen=0.
That client state does not prove the compositor mode. The extra mode was removed
before delivery; only the verified Normal/Fullscreen modes are exposed.
The rule was not changed and no dispatcher was used on the active window. The
window-ui-failure-1793041926965059030.json report and its captures are kept.

Functional acceptance: window-ui-1793042474907145360.json PASS in three processes,
unknown fields preserved and exits 0 with ACK. Final captures: window-preferences-
1793042473963081012.png, window-actions-1793042474075297111.png, window-normal-
restored-1793042474691886649.png. Client dimensions 845×1061/1728×1117, no
outer decoration captured and no OS input. The earlier fixture closing failure is preserved
in window-ui-failure-1793042207114132046.json and fixed without relaxing the lifecycle.
