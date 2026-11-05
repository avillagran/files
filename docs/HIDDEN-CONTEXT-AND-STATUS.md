# Hidden files, context menu and bottom help — 0.0.4

User decision (2026-10-26): **a single persisted global state**
`show_hidden`, false if it does not exist in config.toml. The Space button,
Preferences and Ctrl+H change exactly that state; there are no per-folder/session
overrides. It is saved with corners/window_mode through a typed update
that preserves unknown fields and an atomic write; close waits for the ACK.
Preferences marks the active option and Space toggles Show/Hide hidden files.

## Identity, filter and selection

On Linux/Unix the first byte of the OsStr is checked, without lossy conversion or
I/O on the UI. Groups, List, Spatial and Space share the policy. Fuzzy does not
reveal hidden files. When hiding, batch/focus/anchor are pruned; hits/hover/drag,
preview and inspector size are invalidated. Operations sanitize the selection again and
reject old hidden hits. An already confirmed operation blocks the toggle
until its ACK, so the batch in progress is not silently altered. Refresh does not
restore a hidden selection either. Raw names keep their identity; only labels
may show UTF-8 substitutions. The Windows Hidden attribute is not implemented.

Space reuses the full real report: it does not rescan when the
filter changes. Areas are normalized by complete positive **visible** bytes; the report's apparent
total still includes hidden children/descendants and is labeled.
Visible folders include their hidden descendants. Partial/unknown nodes
stay out of the denominator, with no fake areas or percentages over another total.

## Space and the v3 reference

Contiguous blocks with thin separation, a stable color per name taken from the theme,
computed contrast for text and focus/selection; name on top, size below.
Compact header with Back/Refresh/Cancel depending on state, the hidden-files button and a
real summary. More usable surface and less technical text; no «0 MB resident».
Scan/report limits and the limitations remain in SPACE-MAP.md.

## Context menu

Right click is intercepted before move/resize, selection/double click and opening.
Inside the batch it keeps it; outside it selects the target; the background clears the previous
selection and only offers folder actions. It only uses existing Actions: open the
focus, Space, rename, copy, Trash, Properties; on the background create file/folder,
hidden files, refresh. Multiple Properties and busy operations are disabled.
There are no entries that fake pending features. The menu is bounded to the viewport,
Nerd Font + labels; Up/Down/Enter, Esc/click outside, Shift+F10/Menu key.
The focused target is not presented as opening the whole batch.

## Native text removed and marquee

`canvas.set_label(...)` is removed **at the source**; Frame keeps an empty label.
The help is drawn in its own region of the bottom bar, between the count and FS/free;
errors/saves/operations keep their zones and priority. The tooltip contains
the full sentence. A thread-free status clock sleeps two seconds at the ends,
advances at most 25 times/s and only rotates after finishing the run. Without
interaction/focus, when hidden or without overflow it does not request a repaint. The loop uses User1 damage
limited to the 32 px bar; it does not introduce a global 60 fps loop.

Wayland offers no portable minimization query here: the guard uses
shown/visible and the already stored seat focus, conservatively (it also pauses when
the pointer leaves). Native minimization/gestures and AT-SPI are not validated. The fixture
can set min=max only under --ui-smoke + FILES_TEST_FIXED_SIZE to compare
1000×560; Normal production does not impose floating or geometry on the compositor.

## Validation and limits of the evidence

`python3 scripts/hidden-context-ui-check.py` creates its own data in
~/.cache/files-test, not /tmp. Two instances verify default false, the four
modes, invalid raw UTF-8, the visible toggle/preferences/Ctrl+H action in sync,
persistence true→reopen→false, pruned selection, areas/denominator/total,
batch/background menu, opening existing forms and callback/drain/exit 0.
Captures are the real FLTK framebuffer at 1000×560 with the current theme. The commands
invoke shared handlers; **they are not OS right clicks or keys**. The marquee's
advance uses an injected clock; bytes above the bar are compared.
The pure tests cover throttling, pauses, no overflow, resize and inverted clocks.

The first FAIL is preserved: smoke_test still required order.len==entries.len and
included hidden files in the expectation. The expectation is fixed to the visible count.
The first functional capture without child logging does not prove cleanup of the tree;
it is kept as UI evidence with a sidecar, and logging/identities are required in
the final scene. A suite FAIL caused by ETXTBSY on the open fixture executable
is also preserved: the tests now use an immutable installed interpreter and the script's raw
name as argv, without changing the production path or ignoring errors.

Resources/startup **NOT_MEASURED** because of the freeze. The main PSS footer does not include
Swap/children and does not approve a budget. Historical baseline/startup/clipboard results are still
failed or pending; no 0.0.4 tag or global PASS. The full roadmap continues.

Final validation: 388 Rust (126 core / 44 helper / 218 UI), Clippy workspace
all-targets -D warnings and release PASS. Four open tests are repeated after
stabilizing their interpreter fixture, without changing production code. Scene
`hidden-context-ui-1793059768952742165.json` PASS: eight captures 1000×560,
two instances exit 0 after callback/drain, two helper identities observed and
reaped, unknown field kept. The marquee with an injected clock changes bytes
only in the bottom 32 px; it does not test compositor animation/focus/minimization.
SHA files 101ecb706ed100681e45b21078b160b2d5afba014025d19454dd1586a2298e55,
helper 4f7910cf27b1723b25d17a208b9c2287858decfb0e65abf23c534305aad620c0.
