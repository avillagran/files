# File actions in Space

The complete goal is still active. This checkpoint removes Space's blanket
restrictions in the palette and the MC bar for the actions that the engines
and the keyboard, inspector or menu paths already allowed.

| Action | Availability and authority |
|---|---|
| Copy, move, create ZIP, extract | They open the existing review; opening it runs nothing. Another queued copy does not prevent reviewing the next job. Earlier preparations must deliver their ACK. |
| Rename, create file/folder, Trash, properties, undo/redo | They keep the active mutation/copy barriers and the engine limits. Properties accepts one mark or a focus, without requiring a focus unrelated to the mark. |
| Preview and path | They use the existing focus and existing handlers; no decoders or preview formats are added. |
| Private Copy/Cut/Paste buffer | Keeps the FD capture, revision and controls of the files buffer. It does not prove publishing to or receiving from the OS clipboard. |
| Open | It also requires membership in the current projection, like map navigation. Enter and double click go through the same check. |
| F5 and Ctrl+R | F5 keeps the Space recalculation. MC and the dispatcher share f5_action; Ctrl+R now dispatches Action::Refresh, including the explicit request to retry watching. |

`map_actions` shares availability between the palette, MC, menu, inspector and the
Action dispatch on the map. Painting does not walk the marks, clone names or
query the filesystem; Open uses the projection's bounded cache. Running
checks the marks once before sanitizing/capturing. An invalid or
hidden mark rejects the action, keeping selection and focus; it does not fall back to another file.
Disabled controls explain why; the palette checks again the
owner, generation, the map's revision/hidden state and its exact selection proof.

Operations use the existing marks of the root listing, or the focus if there are no
marks. They may include marks inherited from another view that are absent from a bounded
report. They are not extended to other objects in the report. All/Invert are still limited
to the projection; Clear leaves selection and focus empty. The retained filter is ignored
only in Space. A partial or stopped report does not prevent reviewing known names
from the listing; its bytes/errors are not permissions, existence or authority to operate.
OsString names and each engine's preflight/FD/revisions are preserved.

The palette adds the existing preview. Per-batch sizes and sorting
incompatible with the map keep their restrictions. Native clipboard/input,
AT-SPI, 2x, other OSes, resources/startup and the rest of the ROADMAP are still pending;
the resource/startup freeze and the pause on new previews remain.

Acceptance for source 6d9c5dd and fixture 924f8d4: 1301 normal tests
(447 core, 105 helper, 749 UI), three historical ones ignored; Clippy all targets with
warnings as errors, format and release PASS. Wayland 1793557957164468049,
Xvfb X11 1793557960140989528 and installed Wayland 1793558055069373867 each passed
29 captures, four normal closes with drain and a check of the 15
registered helper identities (14 map, one existing text preview).

The fixture verifies copy, rename, folder creation, prepared move,
Trash and Undo on its own files; properties, ZIP and private Copy/Cut/Paste
are verified as read-only or review/cancel. XDG_DATA_HOME is isolated per app.
The first two 28-capture attempts failed because they did not create the Trash parent
directory: a documented engine precondition; fixing the fixture was enough with the
same binary on both backends. Those FAILs are kept, along with the fixture's JSON macro
depth error (fixed by splitting groups, without raising global limits).

Installation 1793558033383070484, five installed hashes and five backups
verified; app 2cae3b73db4ec56a3ab928d974e585b6d9a2c18537364ff1100722eb8d72d842.
Receipts: `docs/evidence/listing-model/space-map/actions/assessment.json`.
The textual CopyPath route keeps the earlier to_string_lossy implementation:
lossless delivery of non-UTF-8 paths to the OS clipboard is not proven.
