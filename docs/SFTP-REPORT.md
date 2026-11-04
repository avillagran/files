# SFTP diagnostic capture

Status: source d1345fb validated and installed locally; no global PASS or0.0.4 tag.
No new transport behavior, previews or resource/startup experiment.

## Interaction

From the SFTP form, **F4 / Nerd report button** opens a readable captured report.
The form's four fields, selection/cursors and existing Review remain intact.
**Esc / Volver** returns; CtrlW keeps the normal SFTP modal-close behavior.
The report rejects field input/paste and any stale Connect/Disconnect/Check/etc.
control from the form behind it. The profiles view now also rejects hidden
transport/report controls, keeping its own explicit load/review flow.

The report includes:

- The **form draft**, explicitly not an identification of an active connection;
  exact host/user/port spelling/remote directory, current Review validity and form error.
- The already observed local capability summary/error/pending status.
- The last observed transport state, busy/open/writable flags, escaped mountpoint,
  complete retained transport diagnostic and the existing form action shortcuts.

**F5 / Actualizar** captures those already held fields again. It does not dispatch
Inspect, Connect, Open, Poll, Reconcile, Sync or Disconnect, and performs no FS/network
I/O. The normal transport lifecycle may still progress independently behind a
report; its changes do not rewrite the captured text. Recheck/connect remain
explicit actions in the form after returning. Report prose is observation only,
never a source or mount mutation proof.

Arrows/wheel move a row, PgUp/PgDn and Nerd previous/next buttons move overlapping
pages; Home/End and first/last buttons reach the ends. The final page stays full
where possible; resize preserves the first visible source-byte anchor then clamps.
Text wraps at measured glyph advances, preferring whitespace without dropping it;
long tokens fall back to scalar boundaries rather than disappearing past the window.
Controls remain within the supported initial440×440 window.

**CtrlC / Copiar** requests copying exactly the current captured text. The notice
says **copia solicitada**, not that a receiving OS application got its bytes.
Report ID+form stamp guard refresh/reopen/stale copy/navigation actions. There is
no fallback clipboard process, global ownership bypass or read of user clipboard.
No per-selection export is added; this copies the whole bounded capture, with its
explicit truncation marker if present. Its use of the existing plain-text app::copy
bridge does not close the historical native clipboard/input interoperability gate.

## Bounds and retention

One24KiB UTF8 text capture belongs to the form and is freed on report/modal close.
The writer validates UTF8 boundaries and reserves an explicit truncation tail before
appending. Newlines remain logical lines; other diagnostic controls (TAB, CR, ESC,
NUL, etc.) are shown/exported as visible Unicode escapes. Raw mountpoint bytes use
an escaped debug representation, not a lossy replacement path. This is display data,
not an actionable path or a log of external files.

The report borrows fields/status while formatting, never clones an entire diagnostic
just to truncate it. Visible lines borrow immutable text using cached u16 byte ranges;
no per-frame report/string clone, new worker/helper, disk cache or dependency.
Report capture/navigation performs no filesystem/network probe on the graphical thread;
copy still uses the existing clipboard publication bridge. At most text-bytes+1 visual ranges are generated; their
Vec capacity/metadata and allocator effects are additional to24KiB, **not** a total
PSS/Swap budget. Width changes rebuild that bounded index; viewport-height-only
changes clamp navigation. Render boundaries are Unicode scalars; no new grapheme,
AT-SPI/otherOS/native-HiDPI guarantee is made.

The capture can contain host/user/path/error metadata intentionally visible in the
form. Copy is explicit. It adds no credentials/keyring/private-key inspection,
command-line execution or automatic sharing/report upload.

## Validation

Final evidence belongs to `evidence/sftp/report-validation/` and the existing
`sftp-preparation-ui-check.py --report` fixture extension. No separate test server
or new performance instrument. Unit tests cover control escaping, UTF8 truncation,
lossless wrapped slices, resize anchor/clamp, raw mountpoint display, snapshot-vs-live
status, readonly mode, stale actions and preservation of the original draft/Review.

Resources/startup NOT_MEASURED by freeze. Native-input/OS clipboard delivery,
network/mount behavior, helper lifetimes and active FS syscall close are separate
unvalidated scopes in this report-only unit; earlier SFTP-native proof is unchanged.

Final report scene `evidence/sftp-preparation/sftp-report-ui-1793486210147321426.json`
scoped **PASS**:2apps/7realPNG1000×650 and440×440, long own BEGIN–END diagnostic
fully reachable by pages, Unicode/control escapes, fixed capture despite later
status, explicit refresh with new ID, stale copy rejected, readonly input/paste,
form/Review/raw literals preserved, return/reopen and callback root0/drainACK.
Copy is **REQUEST_HANDLER_ONLY_OS_BYTES_UNVALIDATED**; no clipboard read or
receiving-app byte proof. No active helper/storage syscall at close/lifetime proof.

Profile regression `sftp-profiles-ui-1793486235803344634.json` scopedPASS3apps/7PNG,
same final app SHA; local save/reopen/load, lock conflict/exact retry, future file
unchanged.1050Rust349core/91helper/610UI+3existingignored PASS; final Clippyworkspace
alltargets-Dwarnings/release/fmt PASS. No package/lock change or new worker.

Original report positive1793485984728077290 and sidecar retain earlier scalar-only
word splitting at440; final whitespace-preferred wrapping remains byte-lossless
with scalar fallback. Compile/old target-count and Python fixture parse failures
remain in report-validation. Historical global resource/startup/clipboard failures
remain open; no global PASS or0.0.4 tag.

Installation `docs/evidence/0.0.4/installed-checkpoint-1793486330919943223.json`: app `cfaa8f3ac1ac161a477bbb7106e7619873303c53f991711a434fb18a1df83ef6`,
helper `05736746b394413d59df5ecc169341592f66b44d9b895d5388d1b4b4f9e61967`,
HTML `85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4`.
Independent5installed+5backup reread/hash PASS in
`evidence/sftp/report-validation/install-verification.json`. No running user window controlled.
