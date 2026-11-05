# lib-fltk-omarchy toolkit 1ff1adf in Files

2026-11-04, requested by the toolkit thread. Exact export of the tracked
files of `1ff1adfcf4589a490c8f08423df7b6bc1361e3ee` (GitHub main, no
tag; includes glass ff3d315 and pixels_per_unit a967007) into
`vendor/lib-fltk-omarchy`. Files builds without code changes; the lockfile
adds `unicode-bidi` and `unicode-segmentation` (MIT/Apache, see
DEPENDENCIES.md).

It brings: Omarchy tooltips drawn by the toolkit (live tooltips.json,
theme/corners, `#000000` fix), animations that follow the system
(Hyprland `animations:enabled`; `OMARCHY_FLTK_ANIM=on|off`) with fades for
menus/popups/dialogs, a platform layer, i18n, RichText, ListModel, RTL,
editors and the FL_SHORTCUT fix for a null first_window().

Verification (`scripts/toolkit-visual-check.py`, hidden Wayland workspace and
private Xvfb X11, no OS input): the gear's «Preferencias» tooltip
painted by the toolkit; the Preferences menu fades (first frame
≠ last) with animations, identical without them. The checks that compare
menu pixels (toolkit-input, toolkit-menu, popup-media) set
`OMARCHY_FLTK_ANIM=off`. 1640 tests (Xvfb), Clippy.

Fixtures fixed in the same delivery (they are not toolkit API):
`toolkit-menu-ui-check` no longer assumes where Rename is (the menu has had
sections since block 1): Files reports the index and the key presses; and the
block scripts checked `'REV' in line`, which «PREVIEW» always
satisfies, so they never retried when the listing reloaded (an intermittent
video FAIL kept as evidence).
