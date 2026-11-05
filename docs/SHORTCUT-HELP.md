# Shortcut help — 0.0.4

F1 or Preferences → Shortcut help opens its own Painter panel with 38 static
rows: navigation, views, selection/operations and preview/configuration. It
lists only implemented shortcuts, without announcing SFTP, F6/move or other future features.
Actions remains file operations, separate from Preferences.
Help does not run the listed shortcuts when you click their text.

Arrows/wheel and PageUp/Down scroll; Home/End reach the ends. Close/Esc/F1
return to the previous UI, keeping path/selection. Help removes the underlying hits;
context menu and window gestures are not triggered; rows are drawn only inside the
viewport. Reflow bounds the page on resize. No I/O, helpers, timers or data cache.
It does not replace AT-SPI/IME/i18n accessibility, nor does it cover all of F1–F10/terminal here.
The clipboard still has the focus/serial limitation from the earlier validation.

401 Rust 129/44/228, Clippy workspace/all-targets -Dwarnings and release PASS.
help-ui 1793061761953895411 functional PASS: visible Preferences button and F1 action,
pages/ends, four framebuffers 1000×560/800×500 due to a fixture constraint,
no file hits, path/selection preserved, close/reopen/Esc and callback
with help open→drain→exit 0. Method: internal handlers/real framebuffer, not
OS keys/wheel. No preview/du/map helpers requested: root lifecycle.
Resources/startup NOT_MEASURED because of the freeze. No 0.0.4 tag/global PASS.
