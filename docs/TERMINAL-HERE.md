# Terminal here

Linux/Omarchy implementation; other systems explicitly pending. Actions menu,
current-folder context menu, and Ctrl+Shift+P → terminal → Enter all capture the
active browser's directory. A selected child directory is not implicitly entered.
Column/dual focus uses the same Model path as ordinary file operations.

## System launch contract

Files requests `uwsm-app -t service -- <absolute files-helper> terminal <hex>`.
The existing Omarchy application broker manages the external terminal service;
Files does not add a daemon, change default-terminal settings or construct shell
text. The desktop's `xdg-terminal-exec` selects the configured terminal.

The broker's FIFO is read with UTF-8 strict decoding (installed source
`/usr/share/uwsm/modules/uwsm/main.py`, app-daemon input). Sending a raw Unix path
through this transport is invalid. A prototype direct --dir argument was rejected
on source audit before any native non-UTF8 request was sent. Directory bytes are
instead encoded as at most8192 ASCII hex characters (4096 original bytes).
The broker client cwd is `/`; both argv and service cwd thus stay UTF8-safe.

Inside the user service, `files-helper terminal HEX` validates borrowed hex length, digits and NUL
before allocating at most4096 directory bytes, then checks absolute-path semantics.
Odd/oversize/malformed hex, NUL, relative paths and extra arguments all reject before exec. No URL decoding or lexical normalization.
It resolves xdg-terminal-exec only from absolute PATH entries before changing cwd
(PATH bounded64KiB/256entries; executable regular target required). Relative/empty
entries cannot reinterpret the opened directory as a program location. It then
execs `<resolved xdg-terminal-exec> --dir=<raw directory> --` with raw cwd, as direct
argv. This terminal dispatch happens **before** media decoder RLIMIT_AS setup:
user applications must not inherit the decoder budget. All existing media/FS
helper command limits are unchanged. Installed helper location itself currently
must be UTF-8 for the broker; unsupported installations return an explicit error.

## Ownership and UI

Open and terminal share the existing single outstanding launch Job/worker. No
queue, automatic retry, shell expansion or launcher fallback is added. Directory
and executable preflight are bounded; spawn/wait are off UI. A blocking broker can
retain this one worker until Files exits; no syscall/offline deadline is promised.
The terminal is a user application, never an owned decoder or mutation helper.
Closing Files does not kill it or wait for its lifetime. Already admitted user
launches may continue independently. Successful broker exit means **request sent**,
not proof the external window rendered or its shell is ready. Failures after the
system accepted a request are not tracked as Files operation failures.

Success uses a separate static status notice, not operation_message/details:
copy/move rescue diagnostics must remain intact. Next user Action clears that
notice. Unavailable launcher and immediate nonzero exit remain visible errors.

The Actions menu adapts to two columns at short heights so all21 actions fit
at440px, with short uncut labels and complete names/tooltips; no pagination state or preference is added. Text/name tooltips are
application targets, not a claim of native AT-SPI registration or OS input proof.

## Validation scope

Core codec, helper command and UI launch ownership/argv tests; existing open
contract retained. Functional scene uses the existing private Actions/framebuffer
pipe, explicit --attempt and optional --native. It is inert by default/help.
Native mode adds an owned test writer/options via private launcher proxies,
uses the real default terminal/PTY and exact raw cwd, observes only the owned
PID/start/class window, and cleans its own transient service. This is no keyboard,
clipboard, focus-control, resource/startup or complete lifetime experiment.
Reports and original failures are in `docs/evidence/terminal/`.

Final source-stage verification:1151Rust401core/94helper/656UI+3existingignored,
Clippyworkspace/all-targets-Dwarnings/release/fmt PASS. Final scene
`terminal-ui-1793507918398541254.json` scopedPASS9 actual FLTK framebuffers with
FiraCode Nerd Font (nerd=true), all21 short-window actions, Actions/context/keyboard
requests, raw byte directory/argv, nonzero7/unavailable and shared-slot rejection.
Two Files processes exit0 after ACK. The private fake launcher remains alive after
Files closes, then releases and is reaped by the private test subreaper.

Native mode also launched the actual configured Alacritty via UWSM/default-terminal
selection, observed its exact owned PID/start/class mapped, and got tty0+tty1 and
exact raw cwd from the owned writer. That session stayed alive after Files closed,
then exited on test release; only the owned transient unit was involved. Independent
cleanup verified six registered identities absent/reused (same-start Z disallowed)
and unit inactive. No complete lifetime coverage or physical key/click proof.

The first PASS9FB remains as fallback-font scope: fixture PATH had hidden fc-match.
A sidecar records why final fixture exposes only fc-match in addition to its private
launch proxies; missing uwsm never falls through to the host by accident.
