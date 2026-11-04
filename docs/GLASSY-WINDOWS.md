# Glassy windows (Omarchy 4.5)

2026-11-04. User request: Files must allow Omarchy's *Appearance > Windows >
Window Opacity > Glassy Windows* and follow what the system says.

## What Omarchy does

`omarchy-hyprland-window-transparency glassy` copies
`window-glassy.lua` into `$XDG_STATE_HOME/omarchy/toggles/hypr`, turns on
Hyprland blur (`default/hypr/glass.lua`) and gives terminals a see-through
background (`omarchy-theme-set-glass`: opacity 0.65 unless the theme sets
its own), so text stays crisp. GTK apps (Nautilus) instead fade as a whole
through a compositor opacity rule. Files had no rule and FLTK declares every
rectangular Wayland window opaque, so Glassy changed nothing for Files.

## What Files does now

The shared toolkit (`lib-fltk-omarchy` `glass`, origin ff3d315, exported
exactly in `vendor/`) follows the system choice live:

- Mode from the toggle files, same priority as Omarchy (opaque, glassy,
  otherwise transparent). Opacity from the theme's alacritty `[window]
  opacity` (what Omarchy writes, or the theme's own), else 0.65.
- Every surface Files paints (`Painter::rect`/`rounded_rect`) is a glass
  surface: Cairo `SOURCE` with the glass opacity, so repaints never stack
  alpha. Text, glyphs and images keep full opacity, like terminals.
- The window's Wayland opaque region is cleared on the first glass paint and
  restored when leaving Glassy, so Hyprland blurs what is behind.
- Transparent (Omarchy's default) and Opaque stay opaque: Omarchy does not
  list Files among its transparent apps.
- X11/XWayland and RGB24 image/offscreen surfaces stay opaque (no alpha
  visual). No GPU feature or "acceleration" is needed: the compositor blurs.
- Changes are followed live: `ThemeWatch` watches the toggles directory (or
  its nearest existing ancestor until it appears) and calls `glass::reload`.
- Scripted checks (`--ui-smoke`) paint opaque unless they set
  `OMARCHY_FLTK_GLASS` (`off`, `on`, `system` or an opacity).

Known trade-offs: in-window toolkit menus stay opaque; Files' own dialogs,
badges and highlights are surfaces too, so under Glassy they show the blurred
desktop rather than the Files content beneath (as a terminal TUI dialog does).

## Validation

- Toolkit `examples/glass_check.rs` in a real Hyprland window read back the
  ARGB buffer: background/card alpha 166 (0.65), text 255; turning glass off
  restored alpha 255 and the opaque region (`toolkit-glass-check-wayland.json`).
  Toolkit 36 unit (3 new) + 30 UI + 9 doc PASS; no new Clippy findings.
- `scripts/glass-ui-check.py` (fixture listing, hidden workspace, no OS
  input, user toggles untouched): pinned opaque, forced glass and live
  system toggle (private toggles dir: transparent → glassy → transparent).
  The capture drops alpha, so glass surfaces read as colour × 0.65: 99 % of
  the frame changed and 97–99 % matched that exactly; text pixels unchanged.
  Live glassy equals forced glass and leaving it restores the opaque frame
  (status strip excluded: it repaints live PSS every 2 s, observed
  18.0 vs 17.9 MiB in the first failed run, retained). X11 stays identical.
- Files 1621 Rust tests (708/112/801 + 3 historical ignored), Clippy all
  targets `-D warnings`, rustfmt PASS.
- Failed runs kept: missing exit stats (`smoke` is cleared after the smoke
  actions; report flag captured at start) and the status-strip difference.

## Installed

NODE `files` d3ea217f0286 (receipt
`docs/evidence/0.0.4/installed-checkpoint-1793814634317279949.json`), M2
native ARM ee051ecbb0cd (`docs/evidence/glassy-windows-20261104/m2/`).
Five backups and installed hashes verified on both, post-install scenes
PASS, focus retained, settings/defaults and Omarchy toggles untouched. M2
has no pinned HTML engine build cache, so `install-local.py
--reuse-staged-html-helper` (b7a0be7) reuses the staged ARM helper after
checking its adapter recipe, build stamp and the validated report.

Not claimed: the blur as seen on the user's screen (NODE's mode is currently
Transparent and was not changed), resource or startup gates, OS input. Files
windows opened before this install must be reopened once to load the new
binary; after that, switching Window Opacity applies live.
