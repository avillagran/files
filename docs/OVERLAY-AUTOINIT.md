# Overlay redraw must not depend on fixture initialization

The user's report on M2 was correct. The installed ARM binary matched the
previous checkpoint. Its popup repaint fix was incomplete: Files initializes
plain FLTK and the theme; only the opt-in popup fixture called
`fltk_omarchy::focus::install()`. The overlay's pre-flush protection lived in
that keyboard runtime's check. The automated tests therefore enabled protection
that production did not have. Explicit screenshot flushes could also repair a
missing overlay before the assertion.

Reproduction: change the toolkit mapped-window example to plain FLTK, with no
`App::new` or focus-router installation. The previous fix leaves the popup
logically open but a canvas redraw replaces its pixels with the canvas color
(exit101). The same test passes after toolkit9dd9ac2: opening any overlay now
installs one shared, idempotent pre-flush check, independently of the keyboard
runtime. It does not install a keyboard router as a side effect. The old focus
check no longer owns overlay protection.

Files removes its fixture-only focus installation. New fixture support permits
passive buffer captures with no flush/redraw and separate FLTK right-press,
hold and release phases. The media regression loads a real PNG in the inspector,
observes multiple asynchronous refresh cycles, and tests a menu over the
inspector too. These are internal FLTK events and mapped pixels, not OS input.

The earlier test passes remain historical evidence of their limited setup;
they did not establish production correctness. No new performance or physical
input acceptance is inferred. Evidence is in the toolkit's vendored
`docs/evidence/overlay-autoinit-20261104` and Files' popup-media evidence.

Validation closed: toolkit plain-FLTK regression FAIL→PASS, 1620 Files tests,
Clippy with warnings denied, NODE X11 and M2 native Wayland passive media
fixtures before/after installation passed. Both hosts have five verified
backups/hashes; M2 current app SHA begins acb64b049f55, NODE0f7bdae5db35.
See docs/evidence/popup-media-20261104/assessment.json. Existing user windows
were not restarted; reopen Files to load the replacement executable.
