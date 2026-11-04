# Copy/Move destination editor

The existing operation review field uses the common bounded Cursor/Geometry:
UTF8 scalar caret/selection, Shift+arrows/Home/End, Ctrl+word motion/delete,
mouse placement/drag/component selection, Ctrl+A/C/X/Z/Y, undo/redo and horizontal
viewport. All text input preflights4096B/control characters before mutation.
Typing only edits destination; Enter still uses the same copy/move preflight,
queue, captured source and no-clobber engines. Cancellation does not mutate files.

Caret/selection/draft/dialog identity and move review admission/ACK advance a
unique epoch; late clipboard text cannot follow an old caret or another dialog.
Preparation stays read-only and actual destination edits discard prepared plans;
caret motion does not rerun a review. No new clipboard backend or native-input
claim, filesystem work/thread/service/cache/dependency is introduced.

Captured non-UTF8/control-containing destinations remain original PathBuf bytes. Escaped display is clipped explicitly to4096B; operations keep the full captured path. Viewing/selecting
escaped text never reinterprets it as a path. Partial edits reject with an explicit
Ctrl+A hint; only full replacement switches to a new Unicode pathname. That
explicit boundary resets undo: Ctrl+Z cannot reconstruct escaped display bytes as
a filesystem path. Unicode destination undo remains normal. Text Copy copies
presentation only, not an interoperable file MIME offer.

Cursor history is bounded32KiB/16states, Geometry retains exact text<=4096B and
<=4097 advance points. These are local component bounds, not process-tree PSS
acceptance. Resource/startup freeze and new-preview pause remain active. General
conflict replacement/rename/skip policies and operation journal remain ROADMAP
work; this editor does not silently enable overwriting.

## Scoped validation — 2026-10-31

798Rust PASS244core/79helper/475UI,3 existing core ignored; workspace/all-targets
Clippy-Dwarnings/release/fmt PASS. Final logs `target/destination-editor-captured-*`.
Tests cover midline Unicode selection/replacement/undo/redo, rejected4097B input
preserving draft/selection/epoch, word delete, preparing guard, unique dialog nonce,
late caret/reopened-dialog paste, raw partial edit rejection/full replacement undo
boundary, controls escaped and expanded raw display capped with original bytes.

`evidence/destination/destination-ui-1793423889017087207.json` PASS9realFB1000×650/
440²: Unicode destination replacement/undo-redo, late-caret paste rejected,
production pointer end click, oversize1MiB retained draft, actual copy18bytes to
edited target with source hash unchanged, Move review caret retains plan then edit
invalidates it, cancellation leaves other target empty, raw partial/full behavior,
4096B caret visible at440. Root exit0 after drainACK. All9PNG inspected; source
and informational text may clip in440, destination scrolls and action controls fit.

Pointer drag/component selection uses shared production code but no runtime
end-to-end drag assertion here. Move was reviewed/cancelled, not executed by this
scene. No native keyboard/mouse/clipboard/AT-SPI/HiDPI/lifetime/active-helper-close
or global resource acceptance. PSS/Swap/CPU/startup NOT_MEASURED; incidental footer
PSS is not a gate. Historical failures and the full ROADMAP remain open.

Original9FB PASS1793423620909348182 on65d696993b58 retained before the final
control-containing path escape/display-bound fix; final binary is7cadf781a7a0.
Initial workspace warnings (deprecated Atomic fetch_update and test-only helper)
were fixed, logs preserved; no suppressed warnings or new dependencies. The last
workspace suite covers final runtime source; no Rust edit followed it.

Source `9e850a5` installed through
`evidence/0.0.4/installed-checkpoint-1793423949236404441.json`. Five installed
artifacts and five backups independently rehashed exact. App
`7cadf781a7a08c5032c918bf58e8b80008e85ce367ced456375b9eeeb6e8ab1a`, helper
`7e7dd8402607f00992899aebdcd8f428f60253a1614314962b454c8eb21f9aef`, HTML
`85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4`.
Authorized per-user install; no source push/global config/window control.
