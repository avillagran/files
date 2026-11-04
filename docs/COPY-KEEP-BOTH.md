# Copy: keep both

Copy review cycles Stop → Skip → Keep Both with F4 or the Nerd conflict button.
Every new dialog starts at Stop. Move never admits this option. Keep Both numbers
occupied selected-root names, without overwriting or merging directories. Exact
same-folder duplication is allowed only in this mode; descendants of a source
directory remain invalid destinations. Symlinks are copied literally.

`file.txt` becomes `file (2).txt`; `archive.tar.gz` becomes
`archive (2).tar.gz`; dotted directory `folder.txt` becomes `folder.txt (2)`.
Compound tar.gz/bz2/xz/zst/lzma extensions retain their original bytes/case.
Dotfiles without another extension keep their full name before the suffix.
Names retain raw Unix bytes, without URL decoding or display-label round trips.
Stem truncation stays within 255 basename bytes and the existing 4096-byte path
budget; valid UTF8 stems retain character boundaries. An extension that cannot
fit with a nonempty stem and suffix produces an explicit error.

At most 1000 candidates per root: the original and suffixes 2 through 1000.
Exhaustion stops without replacing an occupant. Requests execute sequentially;
there is no reservation of names for later roots. A name chosen for one root may
cause another root to receive a different suffix. Review shows requested names,
not a promise of final names. Results include confirmed actual FD-resolved paths
and a complete numbered-root counter, even when individual details are omitted.

Occupancy probes are observations in the opened destination parent, never
publication authority. Publication uses RENAME_NOREPLACE. A late collision tries
the next name with the same prepared payload, after cancellation checkpoints,
registered staging-identity checks and source/name revalidation. No recopied
payload, new process, per-name heap cache, report vector or persistent cache is
introduced. Numeric name construction borrows the original and uses a fixed
255-byte stack buffer. Existing fixed workspace/report/queue accounting remains;
this is not evidence of lower PSS or faster execution.

This does not provide isolation from hostile staging writers, complete content
hash verification, directory merge, overwrite, fsync/crash durability, Move
conflict resolution, or other-platform support. A numbered copied symlink retains
its literal target; the target is not rewritten to another numbered sibling.
Existing cleanup uncertainty/rescue reporting remains conservative.

## Validation

Core tests cover ordinary/compound/directory/raw naming, UTF8 and path budgets,
1000-candidate exhaustion, same-folder file/directory/link duplication, late
collision without recopy, source modification, cancellation and descendant
rejection. Reports retain numbered totals when path details are omitted. UI
regressions retain default Stop, explicit Skip and Move refusal.

853 Rust PASS (269 core / 85 helper / 499 UI; 3 existing core ignored),
workspace/all-targets Clippy -D warnings and fmt PASS. Logs:
`target/copy-keep-both-final-tests.log`, `*-clippy.log`, `*-fmt.log`.
Release PASS (`target/copy-keep-both-final-release.log`).

`evidence/copy-keep-both/copy-keep-both-ui-1793436602618559696.json`
scoped PASS: seven real FLTK framebuffer PNGs,1000×650 and initial440×440;
keyboard/mouse policy cycling, six numbered roots with actual raw paths,
compound archive/dotted directory/literal symlink; review and paused queue do not
mutate, existing occupants and source inodes/content retained. Same-folder file
copy succeeds; next dialog defaults Stop, Move refuses policy. Root exit0 after
queue drain ACK. No helper requested: root lifecycle only, not helper-lifetime
coverage. All seven PNGs inspected; buttons/counts visible440, long report lines
and additional paths need existing scrolling/pan.

Regression `evidence/conflicts/copy-conflicts-ui-1793436615203947340.json`
PASS eight framebuffers/one root0ACK on the same binary: Stop stops at occupied
root; Skip preserves file/symlink/directory occupants, publishes2/skips2 then
skips4/0published440, defaults reset and Move remains unchanged.

Original scene1793436577392527746 FAIL is preserved with a sidecar: six valid
captures and filesystem checks completed, but harness tried dest-dismiss on a
compact toast with no painted dismiss button. Corrected scene opens the actual
result report first. No production code or deadlines changed for this retry.
No native input/protocol acceptance is inferred.

Source `8623abd` installed through
`evidence/0.0.4/installed-checkpoint-1793436673368581944.json`.
Five installed artifacts and five backups independently SHA256-verified exact.
App `9d41b6fe89860fc6a33a6b4ab7cb771f77e67fcde7c23f21a49cf86d8357ce6b`,
helper `d6bc5579a3072be862d3f62c3ebf88e6ac29ca85c25b520faf071fa2256ebe82`,
HTML `85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4`.
Backup `~/.cache/files-test/installed-backups/1793436673186006201`.
Authorized per-user install; no source push/tag/global config/window control. Resource/startup freeze
remains active; no PSS/Swap/CPU/startup gate. New previews remain paused. Native
keyboard/mouse, clipboard/protocol, powerloss, global/lifetime acceptance and all
historical failures remain open.
