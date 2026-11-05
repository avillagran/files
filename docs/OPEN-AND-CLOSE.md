# Opening files and closing the window

Explicit decision 2026-10-26: files are handed to the system `open` on
Omarchy/macOS. `Command::new("open").arg(absolute_path)` keeps the argument
lossless; no shell, path interpreter or `xdg-open`/opener fallback. Real
directories still navigate inside files; a symlink is handed to `open`, without assuming
on the UI thread that its target is a folder. An absolute path ≤4096 bytes/without NUL is
checked before building the job; only one open is pending. Spawn/wait
run on a worker and errors are visible. Exit 0 proves the launcher exited,
not that a viewer appeared. Today `open` is not in PATH on this host: the capability is
unavailable and the alternative is **PENDING**, without installing or emulating the command.

The launcher is not a managed preview/helper: the external app is not killed when
files closes and its lifetime is not awaited to close the window. A custom `open`
that never exits may hold that single worker until the files process ends;
attempts do not pile up and no timeout is promised. The tests use their own fixture program;
they do not change the global PATH or run data as commands.

## Super+W: findings and limits

The observed installed copy `~/.local/bin/files` keeps SHA256
`c91b9cf20a7f1fa4b48b2e9cbdd6c5b8ebb0302efd9210f7da207577fe295972`,
the same as the folders checkpoint before `3e6cf88`. That version forces
`set_border(false)`. In the pinned FLTK 1.5.23, `Fl_Wayland_Window_Driver.cxx` lines 1136–1138
register an empty `xdg_toplevel_close` callback for an UNFRAMED window. It is a
concrete static mechanism that can explain ignoring the compositor's close;
it is not a reproduction of the reported key press. The current code uses a normal
compositor-managed border, whose libdecor `handle_close` path (1024–1037)
delivers FL_CLOSE/callback. Neither the registry nor the global configuration was patched.

The effective `hyprctl binds -j` shows Super+W press/release `__lua1164/1165` in the
saved observation, not the stock dispatcher described in text. omarchy-x-panel
applies its own periodic binding: its detector compares `function:` while this
Hyprland presents `__lua`, a possible rebind race NOT reproduced as the cause.
Those binds and the plugin were not changed. Only its own window/PID is observed.

`FILES_CLOSE_TRACE=1` logs FLTK callback → request → drain states →
DRAINED. It waits for mutations/rollback/ACK, previews, du, the copy queue and atomic
saves before hiding; errors/rescue or persistence failures stay
visible for explicit resolution. A `win.do_callback()` fixture proves the
second half of the path, **not a compositor event nor Super+W**. The real input path
is still pending if CUA does not enumerate the window. The jev-computer-use skill
forbids improvising another screen control when the target is missing; it was not replaced
with global keys, the active window or a dispatcher acting on other windows.

## Evidence

345 Rust tests (115 core/37 helper/193 UI), Clippy workspace/all targets -Dwarnings and
release PASS. Last specific regression open_job 4 PASS. Scene
open-close-ui-1793056978667673268.json functional PASS, three processes: host missing,
fixture launcher with a literal absolute argv with Unicode/spaces/$() and a symlink, exit 7
visible, a directory navigates without the launcher, each exit 0 through callback/request/drain.
No real viewer is proven. Earlier captures kept a disk PASS but a visual
FAIL because the reason was clipped; the sidecar is preserved, and the summary now shows
«open ausente · alternativa PENDIENTE» and «open falló · código 7» in full.

CUA again returned 0 windows while hyprctl observed its own PID/address.
Super+W/compositor is still PENDING_OS_INPUT, with no bypass or bind changes. App SHA
`81c53b8add62fa941fac3fc9f11f30f9af8441e5d31f6a885dc1db3ad9f186b7`. Resource/startup freeze NOT_MEASURED;
do not use the PSS footer as the tree's budget.

Installed under explicit authorization in ~/.local/bin: checkpoint bdaa5f3,
files 81c53b8add62…, helper 64f7133b20ab…, HTML a5c37981b28b…; full hashes
verified in installed-checkpoint-1793057085303837728.json, backup of the five
artifacts/metadata. The HTML recipe/source stamp was validated before staging. No
other or earlier window was closed, no global install. This update removes
the old borderless installed executable; it does not turn Super+W into a PASS test.

## Shared terminal request slot (2026-11-01)

Terminal requests share the bounded existing external-app launcher with `open`;
file argv/no-fallback policy and FILES_OPEN_OUTCOME trace remain unchanged.
Terminal has its own typed outcome trace/status notice and leaves operation rescue
reports untouched. See TERMINAL-HERE for the ASCII broker/raw-directory bridge,
explicit service ownership and validation boundaries. No viewer fallback added.
