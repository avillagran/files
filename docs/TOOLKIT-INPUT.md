# Toolkit input and Trash checkpoint

The right-click menu remained logically open after release, but a partial
redraw of Files' custom canvas painted over it. The toolkit's generic overlay
API had no stacking protection on plain FLTK windows. AppWindow's existing
stack renderer did not cover that consumer.

The fix lives in **lib-fltk-omarchy**, commits ae2fd91, 77c8b83 and fb581c7
(base 24a3825 / v0.0.10). Live overlays register deletion-tracked handles. The
existing pre-flush check and event dispatcher prepare dirty window stacks;
`fltk_omarchy::app::flush()` does the same for explicit flushes. There is no
new overlay timer and no redraw when the window is idle. All explicit Files
flushes use that shared API.

`scroll::Scroll` is a shared pixel controller used by toolkit List, Table,
Tree and PopupMenu. It preserves the full signed wheel delta, accumulates an
exact bounded destination, eases over 120 ms, drops old travel on reversal,
and cancels on external navigation/drag/position changes. Disabling toolkit
animations makes scrolling immediate. Other row-based popup consumers now
preserve delta magnitude; they are not all animated.

Files adapts that controller to Groups, List, Grid, the Space Map list, both
dual panes, and each visible Miller column. Horizontal column scrolling uses
the actual horizontal delta, including FLTK's Shift-axis remapping. Visible
motion uses the existing shared animation scheduler; changing browser, view,
column scope or normal keyboard/pointer navigation cancels pending travel.

This is wheel smoothing. The pinned FLTK 1.5.23 Wayland driver exposes integer
axis values without axis-source/stop phases. These changes cannot recover
lost precision or establish physical touchpad momentum acceptance. No second
physical inertia model is layered onto unidentified native events.

## Portable local dependency

Toolkit changes are committed in its own repository, **not published or
tagged**. Files consumes an exact tracked export under
`vendor/lib-fltk-omarchy`; `UPSTREAM.json` identifies fb581c7. No cached Cargo
checkout is edited and no absolute sibling path is required. GPU remains off.

Format Files packages explicitly so `cargo fmt --all` does not rewrite the
vendored toolkit's established formatting:

```sh
cargo fmt --package files-core --package files-ui --package files-app --package files-helper -- --check
```

## Trash sidebar

A pinned Papelera entry remains visible in the collapsed and expanded rail,
including the minimum window with the MC operation bar. It opens a dedicated
**read-only home-trash catalog**, also available from the command palette.
Names, available original paths and deletion dates are displayed; arrows and
Page/Home/End navigate, left/right pan the original path, F5 reloads, Esc
closes. It does not expose `Trash/files` as an ordinary mutable folder.
Restore/empty from this view and other volumes' trash remain future work;
existing receipt-based Undo is separate and unchanged.

Catalog I/O runs on the existing bounded AuxIo worker. A missing root is
reported empty without creating anything. Linux directory FDs are pinned and
validated with the existing ownership/private-mode rules, no directory or
metadata symlinks are followed, stored objects are never opened, and metadata
reads are bounded. Orphan objects remain visible with unavailable metadata.
Limits of 4096 records / 4 MiB are explicit, not a silently complete count.
Closing/reloading cancels the old request; IDs reject stale responses. File
commands, clipboard and native drops are blocked while the catalog is open.

## Verification and limits

Toolkit: 33 unit tests, its UI harness and 9 doctests passed. The mapped
framebuffer regression fails before the fix (open menu pixels overwritten)
and passes on Xvfb and native Wayland, including release, repeated canvas
redraw, explicit flush, idle paint count and close cleanup. The Wayland
fixture used a hidden workspace, retained the user's focus and exited.
See vendored `docs/evidence/input-20261104`.

Files: workspace tests cover lossless raw-path metadata, read-only catalog,
orphan visibility, symlink rejection, bounds/cancellation, stale responses,
modal isolation, pinned sidebar geometry and virtual rows. The dedicated
`scripts/toolkit-input-ui-check.py` validates mapped menu pixels after release,
wheel distance/return, catalog reload/last-row access, unchanged fixture bytes
and metadata, and normal close. Final reports are under
`docs/evidence/toolkit-input-20261104`.

These checks use internal FLTK handlers/controllers, not OS pointer input.
No physical touchpad, AT-SPI, lifetime, resource or universal performance claim
follows. Intel comparison remains a measurement of ff7af56, not this input
checkpoint; its reproducible 100k delay still needs profiling.

The final source-verified release (`files` SHA256
`74799b5f56edb0dd1bd77f828afa974c7eb4bb502105b3654f22aa5f20d22eeb`)
passed X11 and native Wayland functional scenes plus the existing toolbar/menu
regression. The old menu harness assumed Rename at index2; Entries already
contains Invert/Clear, so index2 is Open and Rename is index4. That failed
report is retained and the harness expectation was corrected without an app
patch. Two initial Wayland harness failures (geometry assumption and a client
mapping race) are also retained.

Installed on NODE with five verified backup hashes and five installed hashes.
Receipt: `docs/evidence/0.0.4/installed-checkpoint-1793772472114089109.json`.
The installed executable passed the same native Wayland scene, retained user
focus and closed normally. M2/Intel installations were not replaced; their
existing comparison results still describe the prior ff7af56 binaries.

## M2 installation, standing instruction

2026-11-04: user requests every new validated Files version be installed on
M2. AGENTS.md records this permanent project instruction. The same 1edc2cd
source was built natively on the Apple M2 Pro (aarch64). Native Wayland
functional scenes passed before and after installation; all five installed
files and backups were hash-verified, focus retained and own windows closed.
The unchanged ARM HTML helper matched its source recipe. Receipt and reports:
`docs/evidence/m2-update-20261104/m2-installed.json`. The first offline build
needed uncached dependencies; a locked online build completed. An initial
launcher used UID1000; actual M2 UID1001 was detected and used before launching
the app. No new RAM/startup benchmark or physical touchpad acceptance follows.
