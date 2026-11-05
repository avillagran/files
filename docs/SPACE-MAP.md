# Space: size treemap

Approved reference `mockups/v3.html#v-map`, different from **Spatial** (manual icon
positions). The Nerd **Space** button or Ctrl+4 computes the immediate children
with an ephemeral helper; folders include their descendant files. A click
selects the object for the inspector/operations; Enter/double click enters a
real folder, Back/Backspace goes up. Refresh rescans; Cancel and leaving
the view discard the report and cancel/reap the helper. The omnibar navigates
normally; the current filter is not applied to the map, and this is stated explicitly.

The blocks start from the **apparent bytes of regular files by name**,
recursive in folders, not physical blocks nor exclusive disk ownership.
Hardlinks count by name; sparse files keep their apparent length.
Symlinks and special objects are not followed and contribute zero, visible in the
list without an area. There is no «0 MB resident» nor made-up usage from the mockup.
The existing PSS footer is only for the main process; it does not prove the tree's budget.

## Proportion and states

The pure layout splits by complete positive bytes along the longest axis,
with proportional areas and shared boundaries. It does not create a minimum weight for
zero/unknown/error/partial nodes. The painter rounds boundaries to pixels and visually separates
each block by 2 px: those margins/labels are not physical precision for
each pixel. Tiny blocks are also reachable in the list. Labels clip
with their own clip; names are lossily converted only for display,
identity/hits use lossless bytes and the listing generation's index.

A partial node has `bytes=None` and `observed_bytes` as a lower bound; it is
excluded from the area denominator. The measured total only adds complete nodes,
the observed one includes lower bounds. PARTIAL and its cause are not loads in
progress: zero/unknown/error/partial nodes appear in a separate list. Empty shows
0 B without a fake block. A helper error does not keep the earlier report as
if it were current. The selection shows real status/bytes/counts in the inspector,
without triggering a second du. Very long metadata/diagnostics are cut;
full copy/export and a tooltip for each block are still pending.

## Helper, lifecycle and limits

Linux: final root O_DIRECTORY|O_NOFOLLOW, readdir/fstatat/openat by FD, without following
final/intermediate descendant symlinks. The initial open of ancestors has normal
resolution; `link/.` and mountpoints are not a sandbox. No snapshot: content
may change in place during the computation. A directory's identity is revalidated when
it is opened. It only retains results/names of immediate children:

- 2048 nodes, name 4096 B, names in total 256 KiB; diagnostics 512 B/node and 64 KiB
  in total (including the report's reason), before cloning them.
- 1M observed entries, depth 128 and 15 s cooperative. Incomplete folders/files
  never declare exact bytes. Unknown omissions are None,
  not a made-up zero; Some(0) requires EOF of the root enumeration.
- Request 64 KiB, response 4 MiB, writer/reader bounded before growing the buffer.
  Pre-count of canonical names before serde and validation of count/bytes,
  uniqueness/totals/counts/status afterwards. Not a general sandboxed JSON parser.
- One State/worker/child: the last desired destination, cancel and consume ACK/reap before
  replacing in another pump. Supervisor timeout 20 s, kill/wait outside the UI thread.
  Exit/close waits for the drain, without relying on Drop. Preview and du are not launched in
  parallel with the Space helper; a requested preview waits for its cancellation.

An uninterruptible FS syscall can delay kill/reap; the cooperative bound is not
presented as a hard deadline for an offline mount. A single job is kept,
without piling up children/threads after repeated navigation. The JSON protocol parser requires
a UTF-8 root path; a visible error for a non-UTF-8 root, while raw child names are accepted.
Scanner adapters for other OSes are explicitly pending. The bounds
on names/wire/FD/objects are not equivalent to total PSS nor to a P09 LRU cache.

## Verification

370 Rust (121 core/44 helper/205 UI), Clippy workspace/all-targets -Dwarnings and
release PASS. Real helper CLI 9 cases PASS in helper-wire 1793057735444805998:
8:4:2, raw names, root/trailing/file/relative/missing symlink rejected, mismatch and empty.
space-map-ui 1793057823344999138 functional PASS: areas 8:4:2, selection/inspector,
enter/back, zero/empty/partial/error, real viewport in Normal/fullscreen,
cancellation of a 20k walk, leaving to List and callback close with a live helper.
11 PID/start identities observed, all reaped after exit 0 and drain; no
memory/CPU sampler. Sizes are apparent lengths of sparse fixtures.

Method: visible handlers/targets and the FLTK framebuffer, navigation and modes through
shared handlers; no OS events. The fixture explicitly ignores ambient native input
(only with FILES_TEST_ISOLATE_INPUT=1 and --ui-smoke), so that
external interaction cannot navigate outside its data. The normal app does not change with this flag.
The first run failed because of an unregistered subcommand and received ambient
interaction outside the fixtures (listing/metadata reads; no mutation); the
FAIL is preserved, space_map was registered and the scene was isolated. The next one failed
looking for Cancel after it had finished (correctly not visible); it now cancels
an active walk. The initial validation/clippy failures are also kept.

SHA app 6dcb6c9b0079d04aa4d3d59477d91570e6922e020d6e03802b239a2c8f17655a,
helper 737eb1dc23cdd90192c9742762c4c5cb87bca4c45521c51e89bf0259c5b50463.
Resource/startup freeze NOT_MEASURED. V09 visibly functional, accepted with limits;
total budget, raw root/other OSes/offline mounts are still pending, no global PASS.
Later integration: global hidden files default false/synced button, visible
projection with a total that includes hidden files, v3 style and the context menu in
HIDDEN-CONTEXT-AND-STATUS.md. This earlier report keeps its build/evidence.


## Changes after observation

SPACE-MAP-CHANGES.md documents notices of root changes/interruption without
automatic recalculation, the non-recursive scope of watching and explicit recalculation
with F5/Recalculate, which also renews the listing of names/hits. Complete data
does not imply currency or a snapshot. Source/installation acceptance in that contract.
