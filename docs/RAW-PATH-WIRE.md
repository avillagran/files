# Opaque Unix paths in IPC and startup — 0.0.4 (2026-10-27)

Request paths keep JSON strings for valid Unicode. If a Unix PathBuf
is not Unicode, it is serialized as a byte array and rebuilt without loss.
No other engine or image command is added: PreviewImageChecked reuses this
adapter and still checks expected_revision against the real FD/Response.source.
Root and relative use the same representation, without normalizing or decoding URLs;
each command's openat/traversal policies still apply afterwards.

At most 4096 B per path, NUL rejected, size checked before building the PathBuf
or growing the own byte buffer beyond the limit. The whole line is still
bounded to 64 KiB before serde. This does not claim that the JSON parser makes no intermediate
allocations. Old JSON strings are unchanged and the helpers still accept them.
Other OSes get an explicit error for a non-Unicode path until they have an adapter.
OsString transport is not promised in every response/config of the project.

The CLI uses args_os and accepts `--` as the end of options; a folder with non-UTF-8 bytes
no longer causes a panic in std::env::args. Flag values are still text/numbers.
Labels may show the replacement character for bytes with no Unicode
representation; that text never rebuilds the path used for operations. Manually editing
opaque paths in the omnibar and the other raw UI surfaces remain to be validated.

427 Rust 136 core/46 helper/245 UI, Clippy workspace all targets -Dwarnings and release clean.
Tests: a Unicode path keeps its string, a raw path keeps its bytes, raw root/relative,
no URL decoding/normalization, NUL/invalid byte/limit rejection and a raw SpaceMap request
keeping id+path. The first SpaceMap test still expected raw to be rejected and failed;
it was updated to require an exact round trip; the length limit was not removed.

`evidence/raw-path/raw-path-ui-1793071898429792393.json` functional PASS: argv
directory with 0xFE and a PNG with 0xFF, Grid/inspector and full Space with a purple pattern.
Response.source dev/ino/bytes matches the fixture's stat; every RGB of the
bitmap is checked. Two real captures 1000×650, callback→drain→exit 0 and 3 helper
PID/starttime identities no longer present. Internal handlers and framebuffer, NOT
OS input. The initial report 1677948673706 with one capture remains intact; the second
run adds the required Grid evidence, without measuring resources/startup.

serde_json was only added as a dev-dependency of core; it already existed in the lock/UI/
helper, with no new package/license. Core still has no UI. The transport does not replace
the pending FD signature for embedded resources; it does not test raw GUI for every
command nor Windows/macOS either. The original WidgetTracker is still an open cause with
verified protection; the historical global FAILs are intact. Resources/startup
NOT_MEASURED because of the freeze; no 0.0.4 tag/global PASS.


8003944 installed with backup+hash: files 3b4f6e787172e46ab260bd029491caa63f6c973d84839b5d97cb4df59a3aa08c;
helper 7222619952a58c93d2361568e8aa0beea8bee4ee2f3477cf3307720a4d9fac2d;
HTML a5c37981b28b73c95b443122748b46345b398b07b54941ad1d96d9378726e111.
installed-checkpoint 1793072387409896590. Next: FD identity of embedded
resources; paths now preserve bytes, but that does not certify which FD was read.
