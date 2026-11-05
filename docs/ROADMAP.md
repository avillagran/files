# files — full implementation matrix

Matrix partially reconciled on NODE 2026-11-01; history since 2026-10-26. The extended scope replaces the old 0.0.1 limit.
The approved visual design is still `mockups/v3.html`; software Rust/FLTK and the
licenses in `DEPENDENCIES.md` are kept. Nothing is published, installed globally nor
is a remote created by inference. Deliveries only advance 0.0.2, 0.0.3…

States: **pending**, **implemented** (code without full acceptance),
**partially implemented**, **verified** (acceptance with evidence), **blocked**.
No menu, placeholder, contract or core without UI access counts as a complete feature.
The verified 0.0.1 states have the limits in `VALIDATION.md`; the new
whole-tree scenarios must verify them again. This matrix is the source of progress,
not a claim that the pending capabilities already exist.

## Work deliveries

1. **0.0.2**: visible/persisted spatial + selection/zoom/drag; fix stale
   indices and atomic preferences; complete tree gates and timeouts.
2. **0.0.3**: safe operations with UI and transactional rename/preview.
3. Following deliveries: panes/tabs/Miller/treemap, previews/helpers, search/
   archives, mounts/network, real opt-in shaders and adapters. They are split into
   verifiable milestones without changing minor/major; work does not stop when one closes.

## Coverage and acceptance matrix

| ID | Source | Capability | Observable acceptance | Status | Evidence / current blocker |
|---|---|---|---|---|---|
| A01 | DESIGN 1–4, A–C,F–G | Software Rust/FLTK, licenses | Release without GL by default; lockfile and dependency audit | verified | 0.0.1: DEPENDENCIES.md, evidence/licenses.json |
| A02 | DESIGN 2–3 | Base PSS, total budget | 10k/1080p whole tree; goal 50 MB decimal; ceiling 150 MB; baseline gate 60 MiB and analysis >40 MiB; PSS+Swap | partially implemented | last 10k baseline FAIL 56.743936 MB peak/41.455616 MB steady PSS+Swap; the copy scenario 35.503104 MB does not replace the baseline. Freeze without new measurements; no GPU/complete lifetimes |
| A03 | DESIGN 3 | Virtualization/listing/stat | 100k entries without per-file widgets; first batch without statvfs; only visible stat except date/size sort | partially implemented | core/listing.rs; offline mount risk pending |
| A04 | DESIGN 3 | Cold start | Real timeout; p50/p95/max from spawn; fail if any >=100 ms | partially implemented | last external series FAIL: p50 90.644308 ms, maximum 151.978267 ms, 7/10 under 100 ms; freeze without new measurements |
| A05 | DESIGN 5,K | VFS/platforms | Local and remote interfaces; blocking I/O off the UI; cancellation does not pile up offline workers | partially implemented | core/listing.rs; contracts without complete backends |
| A06 | DESIGN 5,N | Ephemeral helpers | Versioned/bounded JSONL; cancel selection; kill/reap the tree; limits and total PSS measured | partially implemented | visible du 22cef80/1cb4880: supervisor/cancel/reap and 100k tree 35.11 MB; preview_text bc8ed30 CLI only; other helpers missing |
| A07 | DESIGN 3 | MIME/icons | 365 extensions; real Nerd glyphs and a readable fallback, without a full catalog | verified | core/mime.rs; 0.0.2/spatial-check.json + FiraCode captures; replaces the vector set per the user's instruction |
| T01 | DESIGN D | Live Omarchy theme/fallback | Watcher re-armed on symlink/replace; partial TOML keeps the palette; visible semantic map | verified | evidence/ui-check.json, hackerman.png, tokyo-night.png |
| T02 | DESIGN H–I | Corners and geometry | Square/rounded/Omarchy inner corners, badges always rounded, stable resize; outer outline documented | partially implemented | 0.0.2: atomic persistence keeps TOML; the outer outline stays in Hyprland, the logical framebuffer does not prove native 2x sharpness |
| T03 | extension | Atomic preferences/session | Read-modify-write keeps unknown keys; atomic replacement and an invalid file preserved with an error | partially implemented | persistence.rs + versioned tab session/ownership and V12 ACK/retry implemented. OWNER-LOCK-RELEASE ebfbd46: creator-PID unlock against inherited FDs, 1290 Rust/fork red→green/UI source+installed preferences 9 FB 3 exits/favorites 11 FB 4 exits/session 6 FB 8 exits scoped PASS. General task recovery/other OSes/integral gates pending |
| V01 | DESIGN E,I | Path/filter omnibar | Breadcrumbs, Ctrl+L, fuzzy, Esc; large→empty navigation before draw without stale indices | verified | 0.0.2: generation/visible/safe get tests; real UI scene. OS keyboard/mouse remain in U03 |
| V02 | DESIGN E,M | Commands/places omnibar | The > and : prefixes run real commands/locations, with visible help and errors | partially implemented | COMMANDS: catalog of 46 through Ctrl+Shift+P/Actions and existing dispatch; private scene 11 FB scoped PASS. Ctrl+L >/: prefixes connected to filtered catalogs (OMNIBAR-CATALOGS, 823 Rust/5 FB scoped); inline completion and protocol/OS input pending |
| V03 | DESIGN E,I | Groups | Cards with immediate children/size/age; Today/week/earlier; aligned numeric columns | verified | evidence/hackerman.png |
| V04 | DESIGN 6 | List and sort | Natural name/size/date, folders first, asc/desc and persistent hidden files | partially implemented | List/natural verified 2ead92c; global hidden default false/persistence and four views integrated, HIDDEN-CONTEXT-AND-STATUS; OS events pending |
| V05 | DESIGN 6 | Grid | Virtualized icons/thumbnails, selection and zoom; UI access | partially implemented | GRID-VIEW+MEDIA-THUMBNAILS: Nerd button/Ctrl+5/zoom/range/hidden/context; 7 real image/video thumbnails + inspector raw Unix/160 px/LRU/tickets, 423 Rust and scene 1793069315038209135 PASS. Persistence/other views/HiDPI/PSS pending; freeze NOT_MEASURED |
| V06 | DESIGN E/decision 2026-10-27 | Columns/Miller | Navigable/virtualized Finder chain, ancestor pruning, keyboard and operations on the focus | partially verified | M26 COLUMNS-CONTRACT: 596 Rust/Clippy/release/fmt; columns-ui 1793124416464738573 PASS 6 FB/chain-pruning/focus/real copy/normal 0 ACK/initial 800×440. Depth 12 and the tag 3→Columns/new tag 5 migration in unit tests. LISTING-COLUMNS-REFRESH ac02206: watching up to 8 roots, sequential reload with a shared worker, raw selection/anchor/viewport, stale tickets and a missing branch; 1278 tests and Wayland/Xvfb X11 scoped PASS. Full persisted chain/OS input/live resize pending |
| V07 | DESIGN E | Dual pane | F9/Tab, independent navigation and operations between panes | partially verified | M27 DUAL-PANE-CONTRACT: two live lists/IDs (pane, tab)/exclusive secondary worker; 603 Rust + Clippy/release/fmt, dual-ui 1793126826929914048 PASS 8 FB/raw F5-F6/queue when closing a pane/normal 0 ACK/initial 800×440. Visible per-pane filters. Persisting both halves, other views/OS input/resources pending |
| V08 | decision 2026-10-27 | Spatial replaced by Columns | Migrate selector/help/session keeping old data; do not confuse with the treemap | replaced by the user | COLUMNS-CONTRACT/V06 in progress; old layouts preserved, without claiming two complete features |
| V09 | DESIGN E | Space treemap view | Cancellable du helper, real tree size, area hit tests, navigation and freeing the helper | partially implemented | SPACE-MAP: Ctrl+4/real Space, areas of complete bytes, inspector/navigation/empty-partial-error/resize/cancel and close with a live helper functional PASS 1793057823344999138; 370 Rust/Clippy/release. v3 style/global hidden button/visible denominator and a total including hidden files in HIDDEN-CONTEXT-AND-STATUS. SPACE-MAP-CHANGES a3f5094: root/interruption notices without automatic recalculation, the button renews listing and report (F5 dispatcher fixed later in d05e51c), visible non-recursive scope, rows without area up to 3; 1281 tests/Wayland+Xvfb X11+installed scoped PASS. Limits 2048 nodes/256 KiB names; SPACE-MAP-NAVIGATION d05e51c: nav cache/reveal/clamp/Page/Ctrl+Shift/rows 65×45 without inflating area, MC 440 with names and contextual F5/tooltip/help, revoked hits; 1284 tests/Wayland+Xvfb X11+installed scoped PASS. All/Invert/Clear/MC Insert and later operations/raw root integrated in SPACE-MAP-ACTIONS.md and the MapMarks/MapActions evidence; the projection limits All/Invert and does not turn sizes into authority. Other OSes/offline/resources/native gates pending, freeze intact |
| V10 | extension | Tabs and history | Create/close/reopen tabs; per-tab back/forward and session restore | partially verified | M22 TABS-CONTRACT: eight active/closed and per-tab states, 513 Rust/UI 9 FB. M23 SESSION-CONTRACT: durable versioned/raw metadata, lock lifetime/read-only, argv/no-session, invalid-future preserved, ACK; 527 Rust/Clippy/release/fmt and tabs-session-ui-1793111266679001110 PASS 6 FB/8 processes. Resources/startup NOT_MEASURED; OS input/other OSes pending |
| V11 | DESIGN 5–6 | Places/mounts | Navigable and persisted XDG/mounts, favorites/recents; errors/safe unmount | partially implemented | XDG/proc mounts + BOOKMARKS hub Ctrl+D/Ctrl+Alt+D, 64 persisted/raw favorites, labels and cooperative merge; 793 Rust/11 FB scoped PASS. OWNER-LOCK-RELEASE ebfbd46: explicit PID-owner release, forks red→green and 11 FB 4 exits source+installed. Native/mounts/management and global acceptance pending |
| V12 | extension | Per-folder preferences | View/sort/zoom/positions recovered without writing into personal folders; hidden files GLOBAL by user decision, no per-folder override | partially implemented; NODE functional | `folder_views` + AuxIo/ACK/retry/close; Save/Forget, restore/tab/dual guards; 18 FB/6 exits X11+Wayland scoped PASS on NODE. NODE installed record 1793515028382395392/5 hashes, installed 9 FB 3 exit 0; OS input/2x/other OSes and full positions pending; `FOLDER-VIEWS-CONTRACT.md`; OWNER-LOCK-RELEASE ebfbd46 fixes flock retention after drop through an inherited FD, without invalidating CAS/busy; 9 FB 3 exits source+installed |
| U01 | extension | Multiple selection | Ctrl/Shift, range, select all/invert, marquee; all views and operations consistent | partially implemented | 0.0.2: Ctrl/Shift, range, all/invert, group drag; marquee and future views/operations missing |
| U02 | DESIGN 6,E | Keyboard/help/context menu | Full F1–F10 shortcuts, context menu, visible help and terminal here | partially implemented | Context menu B3/Shift+F10/Menu, paginated F1, navigation/selection and a catalog of 46 with disabled commands/reason. COMMANDS and evidence 1793404858332671715; handlers/framebuffer. TerminalHere implemented/partially verified in TERMINAL-HERE.md (real broker and raw cwd); QUEUE-REVIEW.md unifies review with an active queue/MC. Full MC profile, accessibility and OS input pending |
| U03 | extension | Accessibility/IME/HiDPI | Composed text/editable clipboard, perceptible focus, 1×/2× sizes, verifiable native driver | partially implemented | CUA 0.28.2 zero windows; internal scenes are not OS events |
| U04 | extension | Open with/properties/permissions/links | Choose an app, real metadata, safe chmod, symlink/hardlink with validation and no overwrite | partially implemented | FILE-PROPERTIES: Alt+Enter/Actions, FD metadata and explicit non-recursive chmod, 341 Rust/Clippy/release and functional scene PASS. Open with/links/ACL/xattrs/multiple/other OSes pending; the system open and the close bug prioritized |
| O01 | DESIGN L | Copy/move | Reflink→kernel→stream, correct bytes/metadata/symlinks, no clobber; compare throughput with cp | partially verified | COPY-UI/BENCH earlier copy, 10k baseline FAIL 56.744 MB kept. M24 MOVE-CONTRACT: native rename F6/context/inspector, review→confirm FD token/global queue/no-clobber/128 roots/FD budget; 537 Rust/Clippy/release/fmt and move-ui 1793113502303715506 PASS 6 FB/inode-metadata-xattrs/raw/symlink/refresh/cancel-close. M25 EXDEV single regular file nlink 1 verified + journal/explicit recovery, 586 Rust/Clippy/release and exdev-ui 1793122272433112203 PASS 3 real cases 63→75/10 FB/close 0 ACK. EXDEV trees/hardlinks/batches, general ACL-xattrs copy/undo Move/general resume pending. Explicit visible SFTP copy: COPY-PUBLICATION/review+queue; native 4 roots/raw/mtime/collision/cancel-partials, without fallback nor copy-delete. |
| O02 | DESIGN E,L | Queue/progress/control | Accessible toast with progress, pause/resume/cancel and cancellation between files | implemented | copy_job/copy_state and toast/forms; 171 tests, corrected controls capture, cancel after progress of 3280 files, ACK/close and cleanup observed; scenario ceiling PASS 35.503 MB with an exit gap, baseline FAIL; durable resume pending |
| O03 | DESIGN L | Conflicts/recovery | Explicit skip/replace/rename; durable journal, resume after a crash and partial cleanup | partially verified | M25 single-file EXDEV WAL and explicit per-phase/proof recovery; new processes for publish/delete/restore gaps in core, real UI publish/restore PASS. General conflicts, trees/batches/hardlinks and other tasks pending; power loss not tested |
| O04 | DESIGN L | Trash/restore/undo | OS adapters; restore the source without clobber; transactional undo tested in fixtures | partially implemented | Linux home Trash on the same FS, visible Actions/Delete and Undo; batch limited to 64, pinned FD/no-clobber/explicit recovery. Fixtures and tests in TRASH-CORE-NOTES; other volumes/OSes, persisted catalog, redo/journal and resources pending |
| O05 | DESIGN 6 | Create files/folders | Editable UI and exclusive creation, Unicode/errors and no overwrite | partially implemented | 0.0.3: real UI with exclusive creation and bytes/collisions/reserved/cancel tested; rename-ui.json/rename-resources-normal-exit.json, batch 2; undoing a creation pending |
| O06 | DESIGN 6 | Clipboard/DnD | Copy/cut/paste native files and local/between-pane DnD, multiple selection | partially verified | M28 raw buffer internal to the window, partial Cut/retry/ACK, scene clipboard-ui 1793130549985863820; OS MIME/real DnD pending |
| R01 | extension | Pure rename plan | Tokens, sequence, literal/case; collisions/cycles; limits before expansion | implemented | rename_plan.rs exported 180002d, bounds 8b57971: 9 tests; no UI, not a complete feature |
| R02 | extension | Visible renamer | Before/after preview, prefix/suffix, extension and dates; regex, errors and explicit confirmation | partially implemented | Real F2, tokens/sequence/literal/case since 0.0.3; scalar UTF-8 regex/captures with caps and preview/apply/undo in 0.0.4 (RENAME-REGEX). Dates, Unicode properties/casefold and a mass-batch budget pending; resources NOT_MEASURED because of the phase |
| R03 | extension | Rename transaction | Revalidate identity, exclusive staging, cycles, rollback/undo and no clobber | partially implemented | rename_transaction.rs exported: 20 tests included Cargo, rollback/cancel/undo-cancellable/diag cap; 0.0.3: UI+disk scene batch 2 preview/apply/undo/redo/close ACK PASS; resources 35.06 MB sampled, not 100k/durable journal |
| P01 | DESIGN J | Space/inspector lifecycle | Space/Esc, arrows change the preview; cancellation by generation and the helper freed on close | partially implemented | Single text/image State, generation/cancel/drain/close verified by 188 tests and the image scene PASS; OS input and text clipboard FAIL, image resources UNVALIDATED |
| P02 | DESIGN J | Images | Reduced decode with pixel/memory limits, zoom and a hard LRU; on-demand thumbnails | partially implemented | PNG/JPEG/WebP helper 1024²/4 Mi pixels, alpha/EXIF/zoom/pan and a real UI scene PASS; observed tree 53.69856 MB but 5 helpers without a sample→UNVALIDATED; baseline JPEG 12/24 MP real IDCT UI/resource 42.36288 MB sampled with 2 verified helpers; M2 MEDIA-THUMBNAILS Grid/inspector LRU + 160 px baseline JPEG/PNG/WebP/video functional PASS 423 Rust; JPEG-DC-PREVIEW 445 Rust/10 vendor lib/Clippy/release and UI 1793078251643976150 PASS progressive 12/24 MP DC 1/8 with the same 32 MiB cap, no AC integrity. PNG-ROWS-PREVIEW 453 Rust/Clippy/release and UI 1793081848018085943 PASS PNG 12/24 MP by rows/Adam7/alpha/EXIF; no full-source RGB. Large WebP/APNG/chroma quality/global cache/other OSes/new PSS pending |
| P03 | DESIGN J | Text/code | First 2000 bounded lines, highlighting, select/copy and encoding/errors | partially implemented | preview_text helper bc8ed30: 256 KiB/2000 LF, UTF-8/UTF-16 BOM, binaries/FIFO rejected; 8 tests/helper and 8 fixtures. Space/selection UI 992fe04, real source capture; OS clipboard FAIL without focus, resource 143eb6d UNVALIDATED helpers not identified; highlighting/HTML/MD render pending |
| P04 | DESIGN J + extension | Rendered Markdown | Styles/lists/tables/controlled local links and bounded images; source text is not a preview | partially implemented | MARKDOWN-PREVIEW: styles/lists/tables/selection/resize, local bitmaps Arc 4 MiB and a contextual resolver before the helper; 242 Rust/Clippy/release, embedded scene 1793034469432638103 PASS. Embedded resources NOT_MEASURED because of the user's phase; the earlier MD UNVALIDATED 43.178 MB 2/8 helpers; P09 LRU/graphemes pending |
| P05 | extension | Safe rendered HTML | Real layout with scripts/network disabled; restricted relative files and limits | partially implemented | HTML-ENGINE-PROBE: pinned litehtml + Gumbo in an optional helper, real Painter/selection/resize/scroll/close; 252 Rust + Clippy/release, functional HTML scene PASS. Approximate metrics with visible clipping; resources/links blocked, allocator/packaging/contrast and measurements pending, NOT_MEASURED because of the phase. M13 inherited FD/effective Revision + watch/reconcile 476 Rust/Clippy/release and html-ui 1793093483630467339 PASS 10 FB/14 helpers gone/notify suppressed/replace-delete-parent recovery; resources NOT_MEASURED freeze |
| P06 | DESIGN J | Multi-page PDF | pdftoppm/pdftotext detected, page navigation, cancel and measure the tree | partially implemented | PDF-PREVIEW: direct sequential pdfinfo/pdftoppm with a pinned FD, visible pages/zoom/pan; 266 Rust + Clippy/release, pdf-ui 1793038247910320945 functional PASS; the original instrumental error kept. M12 expected FD/effective Revision + watch/reconcile 1 s 473 Rust/Clippy/release and scene 1793092913810767063 PASS 8 FB/23 helpers gone/notify suppressed/replace-delete-parent recovery; readonly exit ≠ ACK. M14 per-page extracted text T/pdftotext + ASCII selection handler + UTF-8 unit 480 Rust/Clippy/release/pdf-ui 1793094189643854652 PASS 12 FB/29 logged helpers gone; selection on PDF geometry/Unicode font coverage/complex/password and resources NOT_MEASURED because of the phase, other OSes pending |
| P07 | DESIGN J + extension | Video | Frame/duration and viable playback/seek; ffmpeg/ffprobe/mpv detected, clip and release measured | partially implemented | VIDEO-PREVIEW: direct sequential ffprobe/ffmpeg, pinned FD, 960x540 bitmap/seek/timeline; 260 Rust + Clippy/release, video-ui 1793037769557008693 functional PASS. Software playback at a configured 12 fps/play-pause/EOF/close with an ACTIVE PID and a 4 fps framebuffer clip functional PASS 272 Rust; opt-in audio M11 with the same ffmpeg/Pulse, initially muted; 470 Rust/Clippy/release and scenes 1793091780741012397+1793091967178711219 PASS indices 1/0/hot-toggle ACK/backend retry/close, zero-PCM audio without audibility/sync; budget NOT_MEASURED because of the phase. The 76.6 MiB footer under 24 readbacks is UNVALIDATED for attributing it to playback; VIDEO-PREVIEW UI buffer returned to the single Ready/Recycled slot 454 Rust/Clippy/release and scene 1793082933186610779 PASS 4 FB/pause EOF/active close 9 helpers gone, NO PSS savings claimed; M18 VIDEO-ZOOM 488 Rust and scenes GIF 1793101116101351845/MKV 1793101152229152975 PASS zoom 2 pan 48/36/frames-pause-seek-resume-reopen reset; VFR/4K/visual rotation pending |
| P08 | DESIGN J | Audio/archives/folders/Office | Bounded metadata/playback; listing of archives, counts; limited Office text extraction | partially implemented | AUDIO-PREVIEW: Space with real FFprobe metadata, FLAC/WAV/Unicode/cut and close functional PASS 276 Rust/Clippy/release. AUDIO-PREVIEW expected/effective FD + watch/reconcile and Ready revision 458 Rust/Clippy/release; audio-ui 1793083931390323451 PASS 8 FB/notify suppressed/replace-delete-rename-parent/11 logged helpers gone, ReadOnly close permission ≠ ACK tested in the controller. AUDIO-PLAYBACK 463 Rust/Clippy/release and UI 1793086873981565834 PASS native Pulse/silence 9 FB/play-pause-seek-EOF/hidden-delete/active close 14 logged helpers gone. The native FFplay prototype EOF FAIL/dummy PASS preserved, backend replaced; AS 768 virtual is not PSS. Waveform/cover art; Space folders with strict DU visible/279 Rust and folder-ui 1793040015107090730 PASS; ZIP32 read-only catalog visible/288 Rust/zip-ui 1793040518449068984 PASS; DOCX M21: DOCX-PREVIEW, 500 Rust/Clippy-release-fmt PASS, 10 wire and office-ui 1793105654289278359 PASS Unicode text/selection/truncation/FD identity/reconcile/close 0 ACK 9 logged helpers gone; historical ENOSPC FAILs kept; other Office/audibility/PCM position pending; resources NOT_MEASURED because of the phase |
| P09 | DESIGN J,N + media priority | Cache/limits | FD/revision identity, visible invalidation, LRU by bytes and helpers; measure the tree's PSS+Swap | partially implemented | MEDIA-LIFECYCLE M1: selected image/video checked-FD + epoch/token/inotify/reconcile 1 s, 410 Rust/Clippy/release and scene 1793064138443049803 functional PASS; 13 helpers gone/exit 0. M2 MEDIA-THUMBNAILS Grid/inspector LRU 4 MiB + ticket/160 px/raw byte IPC, notify-suppressed FD reconcile, 423 Rust/Clippy/release and scene 1793069315038209135 PASS. RELATIVE-SOURCE expected+effective root FD/file FD 434 Rust/Clippy/release and wire 1793074260681793818 + UI 1793074268202431350 bounded PASS; DOCUMENT-MEDIA 442 Rust/Clippy/release + scene 1793076116463397564 PASS 2 instances notify/reconcile/root recovery, watcher before the final probes and a hidden candidate; readonly stop false/true separate from the helpers' ACK. MEDIA-THUMBNAILS scene 1793082861888227321 PASS PNG 24 MP/progressive JPEG 24 MP 160 px Grid/inspector/replace/delete/Trash/Space/close 44 logged helpers gone, no measurements. Selected audio expected/effective FD and watch/reconcile 458 Rust + scene 1793083931390323451 PASS, ReadOnly exit allowed ≠ drained. Space LRU/global for every format/other OSes pending; RAW-PATH-WIRE CLI + full raw Unix image 427 Rust/scene 1793071898429792393 PASS; WidgetTracker panic cause open/cancel-drain guard verified; no new disk. Resources/startup NOT_MEASURED freeze |
| Z01 | extension | Archives | Create/list/extract from the UI; traversal, symlinks, expansion/count/quotas protected | partially implemented | ZIP-PREVIEW: Space with a real ZIP32 catalog/count/sizes/raw names/flags, ephemeral helper, 288 Rust + Clippy/release and functional scene PASS; source-only foundation ARCHIVE-OPERATIONS-CONTRACT: shared streaming CRC/quotas + namespace/local-span plan, 1068 Rust/Clippy/release/fmt and 30 helper wire PASS; Extract ZIP32 regular/dirs UI: AuxIo review→Apply→global queue, pause/cancel/owner ACK/raw/sync/no-clobber; 1093 Rust + Clippy/release/fmt and 1793491842976117966 scoped PASS 7 PNG. Same budget, large plans may be rejected; Retry UI cleanup/sync in the retained result: scoped 1793496524673844308 PASS 5 PNG, confirmation/pause/queue rejection/original token/foreign preserved; Create ZIP32 regular/dirs/literal links with reviewed UI→same queue, DEFLATE stream 64 KiB/CRC/raw/UTC 2 s/rwx, no-clobber; 1114 Rust + Clippy/release/fmt and 1793498375771482261 scoped PASS 7 PNG 1000/440. Durable journal/ZIP64/TAR/extracting links/other OSes pending. Extraction e0dd847 historical; creation INSTALLED e5970ff/app 20db1571eb91, record 1793498441702024461, 5+5 hashes PASS. Resources NOT_MEASURED because of the phase |
| S01 | DESIGN M | Name search | Recursive /text/on-demand index, size/date/type filters, navigate results, cancel | partially implemented | RECURSIVE-SEARCH: Ctrl+Shift+F/bounded helper, name/ext/type and UTC size/date filters (SEARCH-FACETS), parents/open/locate/raw/local hidden files and explicit partial state; scene 1793393137003734053 PASS scoped. Counter progress implemented (SEARCH-PROGRESS); visible PROVISIONAL batches implemented with SearchBatches (837 Rust/7 FB+17 FB scoped, SEARCH-BATCHES); SEARCH-CALENDAR F8 Today/Yesterday/7 local dates with a captured clock and explicit UTC, 937 Rust/26 FB/helper IPC scoped PASS. Navigable root SEARCH-SCOPE INSTALLED b405554/app 35c2c4003224, Nerd menu/Alt↑→←, history of 8/32 KiB paths without snapshots, criteria kept/explicit Enter/helper ACK; 1121 Rust and 1793499783655547501 scoped PASS 8 PNG 1000/440. Bounded F9 paging in SEARCH-PAGES and presets 1/24/168 h partially verified; index/global order/custom-natural periods/other OSes/events-resources pending |
| S02 | DESIGN M | Content/regex | ?text in the helper, byte/result/time limits, filters and navigation | partially implemented | CONTENT-SEARCH: Text/F6 and content: exact UTF-8 literal, bounded KMP streaming, FD/revision, lines/raw/counters/partials; 780 Rust/4 CLI/24 FB scoped PASS. 256 B/marked UTF-8 snippets in results implemented (SEARCH-SNIPPETS, 843 Rust/20+7 FB scoped); advanced and guided regex implemented/partially verified in CONTENT-REGEX-CONTRACT.md and GUIDED-REGEX-CONTRACT.md; literal KMP preserved, regex ∅/EOF/bounded multi-line and saved criteria v1/v2. Unicode casefold/other encodings/index/native gates/resources pending |
| S03 | DESIGN M | Incremental index | Authorized roots, mmap/pages, incremental watcher, rebuild/errors | pending | no index |
| S04 | DESIGN M | Compatible semantic search | An alternative whose whole tree respects the ceiling; do not download/launch Gemma 400–700 MB | blocked | DESIGN M acknowledges 400–700 MB: incompatible with the 150 MB ceiling |
| C01 | extension | Checksums/duplicates | UI with comparable hashes, cancellation and bounded memory, confirm destructive actions | partially implemented | SHA256 FD/revision/chunk 64 KiB + batch 128/8 KiB names, single helper/progress/ACK, panel capture/F5/F6/cancel/copy/comparison; 1188 Rust/Clippy/release/fmt and X11+Wayland 10 FB/2 exit 0 drain/6 logged helpers gone scoped PASS. INSTALLED 5da9265/app 4279419032a9/helper 48e017b4b4b2, record 1793518010519338180/5 backups+5 hashes; installed 7 FB/2 exit 0/toolkit popup PASS. `CHECKSUMS-CONTRACT.md`; recursive finder integrated: sizes→SHA/NOFOLLOW/hardlinks excluded/explicit partial/ACK; 1198 Rust+3 ignored/Clippy/release/fmt and X11 Wayland 14 PNG/4 exit 0/8 logged helpers gone PASS. INSTALLED 79a906e/app 5e3f5e851a49/helper faee512719ea, 5 backups+5 hashes and installed 7 PNG/2 exit 0/4 logged gone PASS (`DUPLICATES-CONTRACT.md`); paging/large groups implemented as a follow-up: 1200 Rust/Clippy/release/fmt + X11 Wayland 16 PNG/4 exit 0/8 logged gone PASS; 4096 raw matches/direct wire 5.4 MB. Follow-up INSTALLED a37cbca/app d0401cd1c43e/helper 006fdb1a8f3a, 5 backups+5 hashes, installed 8 PNG/2 exit 0/4 logged gone PASS; group review/Locate integrated as a follow-up: 1203 Rust + Clippy/release/fmt and X11 Wayland 24 PNG/4 exit 0/12 logged gone PASS; INSTALLED 2ec389f/app 441261c2b883/helper 006fdb1a8f3a, 5 backups+5 hashes, installed 12 PNG/2 exit 0/6 logged gone PASS. Raw/control row labels implemented fed0c70, 1206 Rust/Clippy/release/fmt and X11 Wayland 32 PNG/4 exit 0/12 gone PASS; INSTALLED fa24e8e/app bc9871535945, 5 backups+5 hashes, installed 16 PNG/2 exit 0/6 gone PASS (`NAME-LABELS-CONTRACT.md`); header labels INSTALLED 64b7a26/app 0e902ec5f9ef, 1207 Rust/Clippy/release/fmt and X11 retry+Wayland 38 PNG/4 exit 0/12 gone/installed 19 PNG/2 exit 0/6 gone PASS; the initial X11 capture had the left names missing/cause open (`path-labels/validation/render-assessment.json`); other paths/review of mutations pending. Destructive actions and global gates pending |
| C02 | extension | Compare/sync folders | Dry run shows differences/conflicts, explicit execution without loss, journal/undo depending on the operation | comparison and dry-run UI partially verified; Apply pending | DIRECTORY-SYNC-UI.md/CONTRACT and evidence/directory-sync-ui: comparison preserved; Update/Mirror A→B/B→A and Merge, per-row Pending conflicts/decisions, ancestor coverage, filters/proposals/global blocked, report/raw Locate; a single reader/admission/cancel/root close ACK. 1396 Rust 489/112/795+3 ignored/Clippy/fmt/release; Wayland/X11 sync 19 FB 6 roots 12 readers each and comparison 14 FB 6 roots 7 readers each. No Apply/FD manifest/journal/recoverable replacement-removal/queue/recovery/undo; FULL goal/global gates open |
| N01 | DESIGN K + extension | SFTP as a native mount | Connect/disconnect and an OS path visible to other apps; on-demand sshfs/FUSE, capability UI and loopback | partially verified | SFTP controls 1754f83 installed; native actor/registry/recovery/OpenFD/close-retained UI PASS. COPY-PUBLICATION explicit visible review + link date omission, 925 Rust and native 1793457917645506527 PASS copies/raw/metadata/cancel/no-clobber. Default atomic/overall SSHFS FAILs preserved. No global sshfs; local SFTP-PROFILES profiles verified (32/128 KiB, explicit draft); SFTP-REPORT F4 readable capture/word wrap/pages/copy request verified; mount catalog/PID gap/OS delivery of the report/extended diagnostics/offline/kernel D-state/other OSes/resources/native input pending; N01 not closed |
| N02 | DESIGN K | SSH security/reconnection | Verified keys/host keys, keyring, saved locations; offline/reconnect and safe unmount | pending | no real hosts nor authorized secrets |
| N03 | DESIGN K | SMB/UNC | OS mount/adapter and UI, loopback/fixtures, capability and explicit errors | pending | no permanent GVFS/KIO |
| N04 | DESIGN K | WebDAV/FTP | Backends/helpers and mounts where the OS supports them; secure credentials, limits and errors | pending | no backend |
| N05 | DESIGN K | NFS/devices | Use OS mounts, available/offline capability, space/errors without blocking the UI | pending | /proc mounts listing only |
| G01 | DESIGN GPU/shader addendum | Opt-in effects backend | effects=off by default; GL only when ON, hardware renderer checked; software rejected | partially implemented | egl-capability.json: real Apple M2 Pro GLES 3.2; shaders compiled/render fixtures outside the app. Window/Painter/lifecycle/perf missing |
| G02 | DESIGN shaders | Real aura | Port threadaura with working/attention/burst states; ON/OFF comparison, CPU fallback | pending | Painter::effect=false is a stub, not a feature |
| G03 | DESIGN shaders | Real reveal | Transition per region/source, cancel/hidden, CPU fallback; comparative clip | pending | no backend |
| G04 | DESIGN shaders | Real starfield | Procedural GL, idle/hidden stops the loop, capture/clip and total/GPU cost | pending | no backend |
| G05 | DESIGN shaders | Real activity | Per-region glow shader, uniforms and theme, fallback without glow, measured | pending | no backend |
| X01 | DESIGN phases | Windows/macOS | Path/app/trash/mount adapters, build and tests on available hosts; distinguish portable vs tested | pending | historical Linux aarch64 and NODE Linux x86_64 with bounded functional acceptance and a verified local runtime; Windows/macOS without confirmed hosts/tests |
| X02 | extension | Session/crash/watchers | Restore tabs/tasks, atomic persistence, FS refresh and errors without losing the selection | partially verified | M23 SESSION-CONTRACT: versioned tab session with an inter-process lock/read-only/ACK/invalid-future preserved, UI 8 processes PASS. MEDIA-LIFECYCLE: watches + reconciliation/expected FD for previews and thumbnails scoped; the general watcher was added later in LISTING-AUTO-REFRESH and LISTING-COLUMNS-REFRESH ac02206: stable browser/tab/path + column owner, up to 8 roots/16 MiB, serial ACK/defer/viewport/close ACK, bounded functional validation Wayland/Xvfb X11. M24 refresh of source/dest by ID/FD current paths, keeps the selection and cancels everything before barriers. M25 EXDEV journal for a regular file and an explicit catalog/recovery by identity verified; general recovery of tasks/trees/batches, live recursive SpaceMap/other OSes/native input pending; root notices and explicit recalculation in SPACE-MAP-CHANGES a3f5094; resources NOT_MEASURED |
| Q01 | gate for every delivery | Whole-process sampler | PID/starttime identity, reparenting, incomplete never PASS, sampled maximum/steady and Swap | verified | 32 Python tests + baseline ready. complete verifies survivors, not temporal coverage; exits during a read are explicit and unreadable metrics invalidate |
| Q02 | gate for every delivery | 10k/100k/navigation | Total PSS+Swap, sampled peaks/steady, idle/load CPU, UI latency, FDs/threads/processes and release | partially implemented | 10k/100k loaded and layouts verified in 0.0.2; max/steady/Swap/idle CPU/FD/threads/cleanup; repeated navigation/latency/multi-tab/pane missing |
| Q03 | gate for every delivery | Operations/network/previews/effects | Fixtures isolated from personal data; throughput/cancellation/offline and ON/OFF; no concurrent builds/captures | partially implemented | du tree 100k/cancel/reap observed 35.11 MB; 10k UNVALIDATED fast child. Operations/network/previews/shaders in the app still without gates |
| Q04 | report/journal | Evidence per milestone | Commit, complete/pending features, metrics, 2–4 real Markdown images and clips; automatic delivery to the coordinator | partially implemented | Reports sent with commits and framebuffer; checkpoint 0.0.2 tagged; 0.0.3 rename/creation scene and resources 35.06 MB; startup FAIL 150.31 ms, no global PASS |

| U05 | omarchy-app-dev | ES/EN i18n | Automatic locale, selector and translated strings for every view/error | pending | Do not count the Spanish UI as complete i18n |
| T04 | User instruction 2026-10-26 | Omarchy Nerd Fonts | Resolve a Nerd monospace without a catalog, real glyphs/fallback, overflow 1x/2x | verified | FiraCode in spatial-check.json; glyphs reviewed without evident tofu, resize/1x/2x; logical capture, physical 2x sharpness pending |

## Instrumentation and received risks

- `process_metrics.py` / `test_process_metrics.py`: delivered by the coordinator,
  a sampler with identity and reparenting. Peaks are **sampled** maximums, not exact;
  processes that live between samples may escape. It does not include GPU nor the final CPU of
  processes that disappeared. Never PASS if `complete=false`.
- `rename_plan.rs`: the coordinator's pure plan, tokens/sequence/literal/case and
  collisions. Execution/rollback, regex/dates and UI still pending. The standalone
  benchmark does not represent the app; 100k ~87 ms requires a worker and measuring the preview.
- `navigate`: fixed in 0.0.2; it invalidates visible/hits/hovered by generation and uses
  checked access before the next draw. Tests large→empty/index out of range.
- `set_corners`: fixed in 0.0.2; atomic read-modify-write, keeps keys;
  pending desired values/errors survive until ACK and are retried on close.
- Synchronous `path.is_dir()` and `statvfs` can block on offline mounts. An
  AtomicBool does not interrupt syscalls; isolate potentially blocking probes and
  limit concurrency/recovery, without hiding processes or memory.
- Natural sort integrated and released by the coordinator (2ead92c).

## Specific limits of the spatial layout (0.0.2)

- Canonical identity resolved in the worker; aliases share the queue and revisions.
  Renaming/moving a folder changes the identity based on path+dev+ino: migration pending
  in the operations engine. Saves are not a durable journal against kill/power loss.
- Layout at most 4 MiB serialized/16384 manual positions, validation and budget
  before hex-encoding/copying; lossless names. A future/corrupt version is not overwritten.
- A queue of two shared immutable snapshots, one write in flight; coalescing,
  retry and exact ACK. A full queue prevents navigating until the change can be retained.
- Failures stay visible even when changing folders; close waits without blocking the
  UI and offers retry/keep going/close explicitly discarding the pending ones.
- Listing/probe threads limited to 4; the first batch comes before statvfs. It does not solve
  interrupting an offline syscall: process isolation/mount recovery pending.

## Explicit window priority — 2026-10-26

| ID | Capability | Observable acceptance | Status | Evidence / limit |
|---|---|---|---|---|
| W01 | Normal by default and modes | Standard window; normal/fullscreen, reopening keeps the choice without losing fields | functionally verified | window-ui-1793042474907145360: 3 processes/default/fullscreen/normal/unknown fields/close ACK; resources NOT_MEASURED |
| W02 | Native header/grips | Controls excluded, four 24 px targets, real serial, minimum 700×440 and own cursors | partially implemented | Pinned FLTK/libdecor bridge; geometric contract, OS gesture pending CUA |
| W03 | Preferences/Actions | Mutually exclusive menus, configuration only vs operations, Nerd/tooltip/names/shortcuts | partially functionally verified | window-ui 1793042474907145360: targets/handlers/framebuffer and separation; semantic name/tooltip, OS accessible tree not tested |

Resource/startup freeze kept; benchmarks are not reopened for this block.

Native input recheck 2026-10-27: evidence/0.0.4/native-inventory 1793087442753417655
uses the new compositor but cua-driver sees 0 windows with its own fixture mapped before/
after. No OS input/alternative focus; the Jev rule and the pending focus approval intact.

OWNED-HELPERS (2026-10-27): Linux parent-death SIGKILL in 4 spawn supervisors,
464 Rust/Clippy/release and helper-crash 1793088449742352360 PASS 4 roles/direct owned
helpers reaped 9 after a root-only kill, relaunch 0 + ACK. Regressions audio 1793088666411517841/
video 1793089019986522162 PASS. It does not complete the X02 task journal nor global P09/other OSes/
OS input/resources; the test subreaper is not a product daemon nor a PID 1 test.


M11 VIDEO-PREVIEW: opt-in audio/Pulse in the same child and FD, mute by default/M,
absolute track indices, backend notice/retry without invalidating a valid source;
a typed source failure hides the RGB and re-arms the token. 470 Rust/Clippy/release and two
functional scenes PASS plus a standalone audio regression. Child arenas 2 under AS 768 virtual allow
the context, not a PSS gate. P07/P09 partial; resources/startup NOT_MEASURED freeze.


M12/M13 (2026-10-27): the selected PDF and HTML share watcher/token/epoch/
reconcile and the expected/effective FD Revision. PDF uses the same FD in both CLIs, HTML
inherits the FD instead of reopening. Dirty hides RGB/display list and the readonly_gate shared
with audio keeps exit permission ≠ ACK. 473/476 Rust + Clippy/release and scoped PASS scenes,
no PDF text/HTML shaping/global cache nor resources/startup because of the freeze.


M15 AUDIO-PCM (2026-10-27): 481 Rust + 4 Python/Clippy/release, real PCM 440 audio and 880
opt-in video on an own NULL Pulse sink, silence on pause/default mute, 2 FB/live callback/
8 logged helpers gone/module cleanup/default unchanged. Report 1793096793305882328 scoped PASS,
historical route/harness failures kept; no physical sound/A-V sync/clock/PSS/CPU/startup.
It does not complete global P07/P08/P09 nor lift the freeze.


M16 GIF-PREVIEW: animated GIF in Space on demand/single pass (ignore_loop 1), the same
FD engine/generations/stream and thumbnails, 8 MiB/4 Mi pixels/300 s. 485 Rust/Clippy/release,
1793098465429205562 PASS 6 FB/10 helpers gone/active close 0 ACK and inspector 160×90 red/FD signature.
The Grid GIF code path is shared and unit-tested only, not captured; alpha/disposal/exact delay
and PSS pending. It does not complete global P02/P07/P09 nor lift the freeze.


M17 VIDEO-SOURCE-CLOSE: the audio/PDF/HTML readonly gate extended to the selected video/GIF
snapshot/stream, close permission ≠ ACK, admission under the closing mutex and mandatory Helper reap.
487 Rust/Clippy/release + SIMULATED hold 1793099666110053045 PASS 2 roles/2 FB/exit 0 drain FALSE,
GIF regression 1793099692852727198 PASS 6 FB/10 helpers gone/active close ACK 0. Real kernel D-state/
blocked I/O inside the Helper phase/other OSes and global P09 are still pending; freeze intact.
## M18 — video and GIF zoom/pan, 2026-10-27

VIDEO-ZOOM: the same Viewport 1–8×/Ctrl+wheel/+/-/0/drag, frames invalidate pixels without
resetting the transform; seek frees the RGB and keeps 3 floats, a new source/revision resets.
488 Rust 141/58/289 + Clippy workspace all targets -Dwarnings/release/fmt PASS. Scenes
video-play-ui 1793101116101351845 (GIF 11 FB/12 helpers) and 1793101152229152975 (MKV 9 FB/10)
PASS 1000×650/zoom 2 pan 48/36/kept across frames-paused-seek-resume EOF/reopen reset/
active close 0 ACK/logged PID/start gone Z false. Internal handlers, no OS input/clipboard/
2x sharpness/lifetimes, no photographic detail proven by uniform colors.
No 24-frame clip nor PSS/Swap/CPU/startup because of the freeze; P07/P09 still partial.

## M19 — visible preview navigation, 2026-10-27

PREVIEW-NAVIGATION: Nerd previous/next buttons + Alt↑/↓ share an Action,
visible order/hidden/filter; no UI I/O/preload. An end keeps state/revision;
a real change reuses selection/generation/cancel/drain and the last destination.
488 Rust 141/58/289 + Clippy workspace all targets -Dwarnings/release/fmt PASS.
preview-neighbor-ui 1793101665648168454 PASS 5 FB 1000×650/PNG-WebP-raw PNG 0xFF/
hidden excluded/FD source and pixels/real error + back/rapid last wins/callback 0 ACK/
7 logged helper IDs gone Z false. Hit target/accessible name verified; no OS input/
AT-SPI/clipboard/lifetime coverage/active helper close in this scene.
Resource/startup freeze NOT_MEASURED; no playlist/autoplay nor P09 completeness.

## M20 — non-destructive viewer rotation, 2026-10-27

PREVIEW-ROTATION: Nerd/R/Shift+R buttons only rotate the image/PDF/video/GIF view;
direct sampling of the original RGB, with no rotated copy nor modified files.
The numeric transform keeps the rotation across frames/seek and PDF pages/text-return;
a new source/revision/reset 0 removes it. F1 Help adds the real media shortcuts.
489 Rust 141/58/290 + Clippy workspace all targets -Dwarnings/release/fmt PASS.
preview-rotation-ui 1793102443334515413 PASS 14 FB 1000×650/asymmetric 4 quadrants with
checked framebuffer pixels/CW-CCW-reset rotation/video-GIF play/seek/PDF page 2 +
text return/original bytes intact/callback 0 ACK/21 logged helpers gone Z false.
No OS input/native 2x/AT-SPI/lifetimes/active helper close in this scene nor a test of
photographic color; 14 functional readbacks, NOT an isolated measurement of use.
Resources/startup NOT_MEASURED freeze; no global P02/P06/P07/P09 nor 0.0.4 tag.


## M21 — verified DOCX text, 2026-10-27

DOCX-PREVIEW: Space shows the main Word body as UTF-8 through a validated ZIP32 + XML stream,
CRC/DEFLATE/descriptor, limits before allocation, no extraction/resources.
Expected/effective FD Revision and generation; codec/cap errors do not trigger
SourceUnavailable/a retry of a stable revision. CDATA outside the root and non-initial
declarations rejected with a RED→GREEN regression and 10 real wire cases.
500 Rust 141/67/292 + Clippy workspace all targets -Dwarnings/release/fmt PASS;
office-ui 1793105654289278359 PASS 6 FB 1000×650/Unicode 102 B selection/truncation 256 KiB/
raw Unix FD/replace with the same size-mtime/delete-recreate/notify suppressed/callback 0 ACK/
9 logged helpers gone Z false. No faithful Word layout/other Office/OS clipboard/OS input nor
a helper alive exactly at close. Resources/startup NOT_MEASURED freeze, the historical
ENOSPC/baseline/startup/clipboard results intact; no global P08 nor 0.0.4 tag.
Next visible unit V10 tabs and per-tab history: TABS-CONTRACT.


## M22 — instance tabs, 2026-10-27

TABS-CONTRACT: eight active and eight recoverable closed tabs, monotonic IDs,
history/sort/filter/view/zoom/pan/scroll and raw selection per tab. One Model,
one listing/worker; snapshots budgeted before cloning, at most 3 MiB saved
plus the documented reservations. Global hidden and the copy queue keep their semantics.
Undo/copy refresh their captured folder; returning to an inactive tab relists.
513 Rust (141/67/305), Clippy workspace/all-targets -Dwarnings, release and fmt PASS.
tabs-ui-1793109776417107918 PASS 9 FB, independent A/B and quick switching, hidden
pruned without selecting an invisible destination, raw Unix, maximum/reopen, resize
800×500/440 with metadata and the last tab close 0 ACK/helper PID start gone without a zombie.
Three earlier FAILs kept and fixed, assertions not weakened. Handlers/Actions
and framebuffer; no OS input/clipboard nor a helper alive exactly at close.
V10 partial: durable session pending; resources/startup NOT_MEASURED because of the freeze.
Next block: a versioned/bounded durable session of these tabs, without
restoring tasks or operations automatically; dual pane/Miller, move/journal,
mounted network, app shaders and i18n remain open in the matrix.


## M23 — bounded durable session, 2026-10-27

SESSION-CONTRACT: versioned/raw binary format, a borrowed validate before
materializing, a symmetric encoder and limits 4 MiB wire/1 MiB active/3 MiB saved. One
writer with an advisory lifetime lock on the existing AuxIo; another window read-only without
takeover. An explicit argv without restore/autosave unless Save; --no-session disabled.
Invalid/future preserved; a late response does not overwrite the intent/close. The buffer
is retained until ACK, an error blocks per-tick retries, explicit post-rename Unsynced.
527 Rust 141/67/319 + Clippy/release/fmt PASS. tabs-session-ui 1793111266679001110
PASS 6 FB/8 processes normal 0 ACK/save→restore/A-B state/raw closed/argv/read-only/
no overwrite/invalid future bytes. No OS input/clipboard/resources nor crash resume of tasks.
Next unit prioritized by the user: move with F6/context menu/inspector in the global
queue, same FS and EXDEV separate; an identity/no-clobber/cancel-before-publishing
contract/deletion only after a verified copy. Durable journal/conflicts remain
explicitly pending; mounted network/shaders/dual pane/i18n are not abandoned.


## M24 — native move with review, 2026-10-27

MOVE-CONTRACT: F6/context menu/inspector, review through AuxIo and confirm a token of
parents + root FDs on enqueue. Up to 128 roots, 256 admitted FD tokens including
active/ACK, an additional review of 130 FDs; metadata and path charge before admission.
RenameNoReplace, literal/raw source and ancestor control by FD before Apply;
explicit published/verified against a race, no cleanup of an unverified entry.
No global rollback: earlier roots stay moved after cancel/error.
537 Rust 148/67/322 + Clippy/release/fmt PASS. move-ui 1793113502303715506 PASS 6 FB,
review/pause without publishing, four roots with inodes/bytes/metadata/xattrs,
refresh A/B without losing the target's focus, no-clobber collision, /proc EXDEV in preflight
rejected and the source kept, cancel/close while paused 0 ACK. Handlers/no OS input;
cancel after roots only through the core callback; resources/startup NOT_MEASURED freeze.
O01/O03 still partial: next a verified EXDEV manifest, attrs/durability
and a safe journal; no copy+delete fallback nor a current Undo Move. O06 does not advance.

## M25 in progress — EXDEV transfer and recovery, 2026-10-27

Core compiled: effective inventory/hash/metadata, a single-root WAL and an executor for
regular nlink 1 files with durable staging/publication and individual deletion. Explicit
recovery reopens FDs and renews barriers, it does not resume silently. 191 core PASS +
Clippy core clean; public transfer FS 63→75 and three new-process scenarios
PASS (exit 73 at the publish/delete/restore gap). Details/limits/evidence in
EXDEV-JOURNAL-CONTRACT and exdev-core-foundation.json. No EXDEV UI, no installation,
O01/O03/X02 not complete: trees/hardlinks/batches/visible recovery are still WIP;
resources/startup NOT_MEASURED because of the freeze. M24 installed intact.


## M25 — visible checkpoint, 2026-10-27

It replaces the earlier WIP for EXDEV of a regular nlink 1 file and explicit
recovery. 586 Rust 193/67/326, Clippy/release clean; scene
exdev-ui-1793122272433112203 PASS 3 cases FS 63→75/10 FB/close 0 ACK/stages absent.
A readable source→destination catalog, intent proof and ownership until admission;
complete controls in the initial 440×440 window, long texts clipped. Live
resize UNVALIDATED and the FAILs/sidecar preserved. EXDEV-JOURNAL-CONTRACT includes
local accounting, not a global measurement. Resources/startup NOT_MEASURED freeze.

Explicit user priority: PAUSE extensions of previews/media, keep
what is installed and continue with features/UI. Next visible unit: dual pane,
then cut/paste and DnD on the global queue, extend operations and SFTP as an
OS mount. App shaders, i18n and the rest of the matrix remain open. Do not close
O01/O03/X02 because of this single-file scope; trees/hardlinks/batches need a ledger.


## M26 — Columns replaces Spatial, 2026-10-27

Explicit user priority carried out: Ctrl+3/selector/--columns and the --spatial alias
→ Miller/Finder; Space/treemap stays. One Model + active worker and another reusable
lookahead worker, caches without engines. Eight retained listings with a sliding
window (no depth limit), pruning by ancestor and separate focus/source.
The legacy tag 3 migrates without icon zoom/pan/scroll, the new tag 5 keeps scroll.
Old layouts not deleted. V06 partially verified, V08 replaced by decision.
596 Rust 194/67/335 + Clippy/release/fmt and scene 1793124416464738573 PASS 6 FB/byte copy/
pruning/normal 0 ACK/initial 800×440. A deferred, cancellable inactive menu with a
raw name/generation proof; payloads/modifiers/depth 12/migration in tests.
Earlier FAILs kept; the journal lock owner does an explicit unlock only in the creating PID, with a
duplicated-FD regression RED→GREEN. No OS input/live resize/resources/startup/global PASS.
Dual pane next: F9/Tab, captured source, F5/F6 to the reviewed opposite destination,
single queue/journal/supervisors. New previews paused and freeze intact.


## M27 — visible MC dual pane, 2026-10-27

F9/Tab and independent navigation with source/destination captured by F5/F6. One active
Model + a movable Browser, one queue/journal/media/AuxIo; a secondary worker shared
through exclusive transfer with Columns. Tab groups with IDs (pane, tab), 3 MiB
shared metadata including the hidden one, 32 MiB of local data without a PSS claim. F9 off frees
invisible listings; closing a pane does not cancel the queue. The session covers only the focused group
with an explicit notice, dual persistence pending. 603 Rust 194/67/342 + Clippy/release/
fmt and dual-ui 1793126826929914048 PASS 8 FB/raw copy and move/normal 0 ACK/initial 800×440.
No OS input/live resize/resources/startup/global PASS. New previews paused.
Next: cut/paste and DnD on the existing engines, extend operations and mounted
SFTP; EXDEV trees/hardlinks/batches, app shaders/i18n and the rest remain open.


## M28 — visible Copy/Cut/Paste, 2026-10-27

Ctrl+C/X/V, context menu/Actions and a reviewable raw destination on the single queue. Capture
through AuxIo/Source FD + effective Native/EXDEV tokens, the original subset for retry,
a live Control and typed cancellation; Cut only consumes what is confirmed per generation.
Joint clipboard budget of 2 MiB/joint local FDs, no PSS claim.
624 Rust 203/67/354 + Clippy/release/fmt, scene 1793130549985863820 PASS 8 FB/2 normal 0 ACK,
Copy 0xFF→dest 0xFE, Cut 2 partial/retry/cancel/replacement, a new buffer survives the ACK.
The canonical FLTK MIME constant fixes the false Rust query caused by a pointer: own text
immediately and after the event loop exact, NOT a test of focus/serial nor the external clipboard.
Initial menus 800×440 without a toast; earlier FAILs/sidecar preserved. O06 partial:
M29 native MIME/DnD with a real gesture/serial and bounded bytes, contract before wiring.
No data-control as a focus fallback. Previews paused and freeze intact.


## M29 — native MIME/DnD active, prior contract

NATIVE-TRANSFER-CONTRACT.md contract: wl_data_device/bridge with gesture + real serial,
own proxies/listeners coexisting with FLTK, offers/tokens and bounded FDs before
alloc; no FL_PASTE URI text nor data-control for focus. A raw codec exactly once,
captured destination, existing queue/proofs. External DnD initially negotiated as COPY;
finished does not authorize deleting nor duplicates a delete, internal Move through its own ACK.
M29a WIP: codec/clipboard bridge/AuxIo/review implemented, 637 Rust + Clippy/fmt
passed; OS events/MIME not validated. One common parent; KDE/multi-source and external
DnD pending (FLTK negotiates in parallel; needs a reproducible opt-in hook).
The private M28 does not close O06. New previews paused/freeze intact, the rest of the ROADMAP not abandoned.


## M29b — DnD receiver in integration, 2026-10-27

Opt-in FLTK 1.5.23 hook per own window, a reproducible patch and a link guard
by hash; a COPY/URI receiver and destination per pane/column without stealing focus. 642 Rust
(208/67/367), 3 ignored; Clippy/fmt/release exit 0. C++ test with simulated proxies and
clipboard-ui 1793135944759735086 scoped PASS 12 PNG/2 root 0 ACK: private pipe→
AuxIo→review→cancel/retry→queue/raw copy; source/selection kept.
Native Wayland/OS input/interop UNVALIDATED, sender/KDE/multi-parent/other OSes
pending; O06 still partial. Resource/startup freeze and the preview pause intact.


## M29c — partial DnD sender, 2026-10-27

A COPY sender on its own window: it keeps the permission of the original press, waits for an
8 px threshold, captures raw names on press and verifies the Source/encodes in
AuxIo. Before requesting `start_drag` it checks window/seat/epoch/original serial,
expiry and the left button; releasing or cancelling invalidates before
taking the Model. A multiple selection keeps its batch when dragging and a click
without a drag is resolved on release. The joint reservation includes the desired
clipboard, proof, payload and published source; it is held until ACK or cleanup
(also bounded by a deadline). The source announces only COPY; `finished` or
`cancelled` never delete files nor consume a Cut. The own drop reuses the
existing review and queue.

648 workspace tests (208 core, 67 helper, 373 UI; 3 ignored), Clippy/fmt/
release PASS. A C++ fixture uses simulated proxies: COPY/serial/surface in an
already admitted helper, permission/epoch/deadline rejections and `finished` marks cleanup; it does not
prove the compositor's grab/focus nor a complete teardown. Scene
`clipboard-ui-1793136958438850356.json` bounded PASS: 13 framebuffers, real Source
and AuxIo, a simulated final dispatch, Cut kept and two closes with ACK.
There is no OS input nor proven Wayland interop; O06, KDE, multi-parent and adapters
remain open. The first scene `clipboard-ui-1793136933634122245.json` FAIL:
pasting own text after the event loop did not arrive. The PASS repeat neither explains nor
erases the failure; a stable global clipboard is still not proven. Resources and
startup NOT_MEASURED (freeze); new previews paused.


## M29d — correlated KDE, 2026-10-27

The KDE import requests `text/uri-list` and `application/x-kde-cutselection`
from the same offer and epoch, with a single ACK after validating intent, Source and
destination. Two pipes are drained concurrently with a common 3 s deadline; the URI list is
limited to 1 MiB and the metadata to exactly one byte `0` or `1` with EOF. An error,
cancellation, limit or offer change rejects everything without turning Cut into Copy.
The bridge's per-phase maximum is 8 FDs: four sends plus four transient
ends; after opening, two readers remain, which are closed before capturing the
Source. Other workers keep independent reservations.

652 workspace tests (208 core, 67 helper, 377 UI; 3 ignored), Clippy/fmt/
release PASS. A C++ fixture with simulated proxies verifies two MIME types of one offer
and rejection when the selection changes between EOFs. Seven transport tests include
both orders, invalid metadata, the limit, the deadline and cancellation after the URI
EOF. Scene `clipboard-ui-1793137466717410666.json` bounded PASS: two private
pipes, a simulated ACK, Cut opens the Move review, cancel keeps the source, two
framebuffers and exit 0 after ACK. Real KDE input/interop are still UNVALIDATED;
multi-parent, real DnD and O06 remain open. The intermittent text FAIL
`clipboard-ui-1793136933634122245.json` remains unexplained. Resources and
startup NOT_MEASURED because of the freeze, new previews paused.

## lib-fltk-omarchy adoption — theme, 2026-10-28

`lib-fltk-omarchy` v0.0.1 (`50a42927`) feeds the Painter colors,
the Corners preference and the badge radius. Files' watcher validates the files before asking the toolkit to reload;
the `FILES_THEME` fixture override keeps the earlier behavior. The resolved Nerd Font is reapplied on
reload. AppWindow is not used: Files' native libdecor bridge is kept.
Surface-specific colors still use Files values; the toolkit's typographic scale/
spacing and states are not applied to all of Painter.
PopupMenu, OperationToast, TabBar and ShortcutHelp are still pending adoption
with visual and functional parity. Multi-parent is still WIP in `e9ebdcd`.

First build: consistent normalized headers and a single copy of
fltk/fltk-sys 1.5.23 (`docs/evidence/0.0.4/fltk-omarchy-first-build-headers.json`).
A framebuffer scene with internal actions and a private theme: reload by rename,
font.base read and square Corners kept; OS input not tested. 668 workspace
tests passed, 3 ignored; Clippy/fmt/release PASS. Resources and startup
NOT_MEASURED because of the freeze.

### Toolkit v0.0.1 component clashes

The current Painter menu is kept until PopupMenu limits and scrolls rows
when they do not fit and exposes close/focus: [issue #1](https://github.com/avillagran/lib-fltk-omarchy/issues/1), [issue #4](https://github.com/avillagran/lib-fltk-omarchy/issues/4).
TabBar, ShortcutHelp and OperationToast keep the existing implementations
because of tab scrolling, help limits/Home/End and toast labels/localization:
[issue #2](https://github.com/avillagran/lib-fltk-omarchy/issues/2).
Multiple selection, virtual tables/trees, PopupCard, animation and the native gesture
were requested in [issue #3](https://github.com/avillagran/lib-fltk-omarchy/issues/3).
The libdecor bridge is not replaced with Hyprland IPC. These are toolkit parity
pending items; no components were declared adopted.

### Toolkit reload and partial writes

Until [issue #5](https://github.com/avillagran/lib-fltk-omarchy/issues/5)
lets the toolkit reject invalid TOML, Files keeps its watcher and validates
the files before requesting `theme::reload`. The last drawn palette is
kept if reading or parsing fails. The validation and the toolkit's internal read
are not atomic against a concurrent writer. The scene
`toolkit-theme-ui-1793181731296656632.json` verifies a truncated write,
a retained framebuffer and a later theme replacement; it does not validate OS events.
669 tests (223 core/67 helper/379 UI; 3 ignored), Clippy/fmt/release PASS;
resources and startup NOT_MEASURED.

## lib-fltk-omarchy v0.0.2 — 2026-10-28

Git pin `210e0d64`, single FLTK/fltk-sys 1.5.23 with the existing features;
the header guard and evidence `fltk-omarchy-v002-headers.json` PASS. Theme and
TOML validation stay; libdecor remains. 670 workspace tests, three
ignored, Clippy/fmt/release PASS. The framebuffer theme scene PASS and
`toolkit-v002-focus-fixture.png` tests the ring with `focus::press` in an
**isolated toolkit test window**, not in Files nor with the OS keyboard.

The toolkit's router is not yet enabled in Files: a single canvas contains
both panes and the existing Tab; focusable pane controls and a
virtual cursor are needed before removing that path without losing shortcuts. Migration and clashes
with the omnibar, views and menus: `FLTK-OMARCHY-KEYBOARD-MIGRATION.md`, issue #3.
PopupMenu still has no scroll for rows that exceed the viewport and no public
close/notice API; the previous focus is restored in v0.0.2. Issues #1/#4 commented on;
earlier menus kept. Resources/startup NOT_MEASURED; no global PASS.

## 2026-10-30 — Usability, local search and directory sizes

New user priority covers both MC/keyboard workflows and ordinary mouse use.
`USABILITY.md` records the broader programme without reducing ROADMAP scope.
Local name filtering now supports AND terms, exclusions, full-name globs,
literal phrases, extension alternatives and entry kinds with bounded parsing.
Search/clear hits, Ctrl+F, MC-style Insert and F3/F8 aliases reuse production
handlers. Ctrl+Space exposes directory-only calculation with entry counts;
refresh invalidates previous results. Directory traversal avoids redundant
metadata reads for directory/link entry kinds, and fuzzy scoring no longer
allocates a lowercase copy of each Unicode filename. These are source-level
changes, not a measured performance claim.

679 Rust tests, Clippy/release/fmt and a six-framebuffer functional scene pass;
see USABILITY for exact evidence and installation hashes. S01 recursive filename
search remains pending, as do size/date/content facets, per-row folder sizes,
complete MC navigation profile and native input acceptance. Resource/startup
freeze and preview expansion pause stay active.

## 2026-10-30 — Recursive filename search

Second usability slice: Ctrl+Shift+F, Actions/background menu and a dedicated
keyboard/mouse panel with captured root, local hidden scope, basename+parent,
open/locate and bounded virtualized rows. FD-anchored no-follow Linux traversal
stays on the root mount; results use raw IPC paths and explicit partial states.
One shared helper supervisor and cancellation/ACK admission; root close drains.
Modal DnD/file-clipboard/popup rejection and generation-safe text paste added.

688 Rust tests and Clippy/release/fmt pass, eight framebuffer final scene scoped
PASS; see RECURSIVE-SEARCH.md for reports, caps and limitations. Resource/startup
NOT_MEASURED; new previews still paused. Content/date/size search, pagination,
ignore rules, size rows/progress and other ROADMAP work stay open. No global PASS.

## 2026-10-30 — Measured folder rows and queue

Ctrl+Space/Ctrl+Shift+Space, inspector and menus now drive a bounded current-root
queue, one shared admitted helper, streaming partial counts and measured rows.
List/Grid/Groups/Columns/dual matching-root displays and size sorting consume
observations, with unknowns last. Refresh/owner/navigation/mutation invalidation,
late cancel and final-after-reap semantics are documented in FOLDER-SIZES.md.
External deep changes still require refresh; allocated space/hardlink dedup,
recursive cache watchers/mount policy/offline syscall handling remain separate.
Resource/startup freeze and preview expansion pause remain active.

## 2026-10-30 — Selection tools and MC focus

Filter-to-mark/show-all Ctrl+Shift+A, Ctrl+Shift+D true empty operation selection,
Actions/context equivalents and marked footer counts supplement Insert/Ctrl+A/I.
Invert updates one set; All keeps focus; refresh restores a surviving marked focus
when a filter removes its old name. Selection remains current visible scope with
raw operation names and existing owner/budget rules. SELECTION-TOOLS.md records
validation. Classic MC full profile, general keyboard/accessibility/native input
and the broader UI/tools/search/operations ROADMAP remain open; freeze unchanged.

## 2026-10-30 — Coherent keyboard navigation and viewport rendering

Home/End, pages, Shift ranges and Ctrl independent focus now share a browser
handler across List/Groups/Grid/Columns/dual. Groups model order follows its painted
date sections; five derived boundaries replace per-paint index vectors and full
section scans, while List indexes visible rows directly. Focus strips, partial-row
reveal and stale/empty anchors are addressed. KEYBOARD-NAVIGATION.md records limits.
Full MC profile, command catalog/palette, accessibility/native input and overall
operations/search/memory/startup goals remain open; no resource freeze lifted.

## 2026-10-30 — Editable path field

Cursor/selection/scalar-safe midline insertion/delete, text undo/redo, mouse
selection/component, horizontal scroll and confirm/cancel buttons replace
append-only path editing. Clipboard target includes cursor revision; raw/control
paths use explicit bounded local URIs, exactly-once user confirmation decode.
PATH-EDITOR.md records caps and gaps: graphemes/bidi/native IME/context menus/
completion/filter-caret/platform acceptance and overall resource/startup remain
open. No new preview work, worker, timer, dependency or global config change.

## 2026-10-30 — Search size/date predicates

Recursive search accepts checked apparent-file size ranges/comparisons and UTC
modification date boundaries, composable with existing name/ext/type/negations.
Metadata is requested only after cheap candidate filtering; ordinary queries do
not add lstat calls. Unknown metadata is excluded even under negation and marks
results partial. SEARCH-FACETS.md records semantics and limits. Relative/local
dates, unit/calendar pickers, content search, streaming progress/index and global
performance/native-input acceptance remain open. No resource freeze lifted.

## 2026-10-30 — Visual search criteria

The search drawer adds bounded minimum/maximum sizes and UTC since/before fields,
mouse targets and F4/Tab/ShiftTab/CtrlF/CtrlA/Enter/Esc keyboard policy. Criteria
AND with advanced text and survive collapse; clear preserves the advanced query.
Compilation is bounded before helper admission, and a separate edit epoch prevents
paste redirection across inputs without discarding in-flight work on focus alone.
SEARCH-FACETS records exact semantics. Rich caret/IME, picker controls and local/
relative dates, streaming/content/index search and overall native/resource goals
remain open; previews paused and resource/startup freeze unchanged.

## 2026-10-30 — Discoverable commands

COMMANDS adds a 46-action fuzzy catalog opened by Ctrl+Shift+P or the Actions
menu, keyboard pages/mouse rows and explicit disabled reasons. Dispatch reuses
normal operations/queues and revalidates owner/generation/exact selection; stale
painted targets carry an input epoch. Selection backing is <=64KiB, query256B,
no file-name clones or per-paint full selection scans. Native menus and compact
fallback retain separate preferences/actions. Registry matching does not execute
shell text. This is not a full MC profile/terminal, arbitrary command launcher,
new transfer engine, native-input or resource/startup acceptance; the full roadmap
and pause/freeze remain intact.

## 2026-10-30 — Bounded live search counters

SEARCH-PROGRESS adds a separate legacy-compatible NDJSON search_stream command,
monotonic provisional counters and one latest slot using the existing supervisor.
No paths/actions before final root validation and reap. Open/Locate/Stop reflect
availability; Enter waits for current work and refreshes completed empty searches.
A40k private fixture verifies positive progress and cancel/new-intent plus previous
controls/raw/partial behavior. Streaming result batches, index/content/pagination,
platform/offline/native-input and resource/startup acceptance remain pending.

## 2026-10-30 — Size queue pause and selective retry

Acciones and command catalog expose Pause next/Resume/Retry failed sizes. Pause
finishes the active helper and gates the next admission; retries preserve pause,
good totals and manually cancelled rows. Queue/root/ACK contracts unchanged.
Paused rows and status counters take precedence over rotating help/free-space;
compact440 status remains separate from incidental selfPSS. Catalog now49 actions,
unavailable controls retain reasons.748Rust/Clippy/release/fmt and final ten-FB
scoped size scene in FOLDER-SIZES.md. Earlier clipped footer preserved as visualFAIL.
No new engine/dependency/worker or memory improvement claim. Native input/global
resource/startup/snapshots/mounts/offline/size catalog detail and full ROADMAP open;
freeze and preview pause unchanged.

## 2026-10-30 — Rich recursive search draft editing

Query+four criteria now share path scalar-editor mechanics: caret/ranges, Ctrl-word,
Delete/undo/redo, mouse point/drag/double component and horizontal viewport. Input
vs result focus keeps Home/End/Enter predictable, Tab/CtrlF returns to input;
cursor changes preserve valid results/worker but invalidate late paste targeting.
Bounded point/exact-text cache avoids width measurements on progress/caret-only
paints; binary pointer lookup, no timing/PSS claim. SEARCH-EDITING.md records754Rust,
Clippy/release/fmt and24FB final scoped scene, known CJK/micro-hint limits.
Ordinary filter still pending this editor; IME/graphemes/native/global resource
acceptance and restROADMAP remain open. Freeze/newpreview pause not lifted.

## 2026-10-30 — Local filter text focus

Browser filter now uses bounded Cursor/line_field geometry: CtrlF retains/selects
query, live middle edits and per-field undo, text mouse selection and async paste
epochs. Text vs file shortcuts split, Enter/Tab return list, Backspace in list goes
parent, CtrlShiftA/D MC selection kept. Path pencil remains mouse-accessible with
query; pane/tab queries persist while ephemeral edit context/history drops.
758Rust/Clippy/release/fmt and final12FB+samebinary24FB search regression documented
in FILTER-EDITING.md with3fixtureFAILs/guide visualFAIL and preserved evidence.
No native/global resource acceptance, new engine or measured saving; freeze and
preview pause unchanged. Minimum informational toast, other responsive widths,
IME/graphemes and full tools/operations/search/size/ROADMAP remain open.

## 2026-10-30 — Folder-size queue tool

Actions/Ctrl+Alt+Space/catalog expose the existing queue as a mouse/keyboard tool:
per-row cancel/retry, pause next admission, stop all, failures-only retry, visible
batch and wrapped diagnostics. Borrowed records, monotonic attempt IDs and captured
pane/tab/root/generation/epoch reject obsolete controls and old completions.
764 Rust tests plus Clippy/release/fmt; scoped14FB manager and11FB catalog scenes on
the same final binary. Details and limits in SIZE-MANAGER.md/VALIDATION.md.
No new scanner/cache/dependency or measured resource claim; native-input and global
resource acceptance, snapshots/offline/mount policy and broader ROADMAP remain
open. Resource/startup freeze and new-preview pause preserved.

## 2026-10-31 — Optional MC operation strip

Persistent Preferences/Ctrl+Alt+B/catalog toggle exposes eight existing F-key
actions to mouse users too. Reserved viewport space, short labels at440, full
accessible names, text-focus guard before pointer blur, current owner/generation
and existing operation preflight; no extra engine/payload cache. See
OPERATION-BAR.md and VALIDATION.md for scoped checks. Full keyboard/native/global
resource acceptance and the remaining operations/search/SFTP/shaders/i18n roadmap
stay open; preview expansion and resource/startup freeze stay unchanged.

## 2026-10-31 — Literal UTF8 content search

Recursive search now combines names/metadata with one exact positive content
criterion. Texto/F6 preserves separate drafts; advanced content syntax remains.
Bounded streaming KMP/UTF8 validation, same-FD metadata+revision postcheck, explicit
binary exclusions/limited/changed partials, first match line/byte. Same supervisor/
cancel/ACK, raw paths and output caps; no new service/dependency/index/cache.
780 Rust+Clippy/release/fmt,4 release CLI cases and24FB final scene scoped PASS;
CONTENT-SEARCH.md records reports/bounds/limits. Regex/case folding/encodings/
semantic/index/pagination/native/global-resource acceptance remain open, as do
operations/SFTP/shaders/i18n/restROADMAP. Freeze and new-preview pause unchanged.


## 2026-10-31 — Favourites and quick folders

CtrlD/CtrlAltD/Nerd star/catalog53 share one bounded favourites/XDG/current-tab
history hub. Labels only, raw paths, existing async navigator and AuxIo worker;
cooperating-window lock/reload/merge, monotonic row IDs/exact expected tuple,
64KiB wire64rows, explicit captured retry/omit and published-unsynced state.
793 Rust+Clippy/release/fmt; four-process11FB scene and same-binary11FB catalogue
regression scoped PASS. See BOOKMARKS.md/VALIDATION.md for preserved failures and
bounds. Native input/global resources/mount management/other platforms remain;
no reduction of operations/SFTP/shaders/i18n/restROADMAP, freeze/previews unchanged.


## 2026-10-31 — Copy/Move destination editing

The operation review field now shares bounded Cursor/Geometry: keyboard and
pointer selection, midline Unicode edits/undo, horizontal reveal and per-dialog/
caret/draft paste epochs. Source capture/preflight/global queue/no-clobber stay;
actual destination edit discards a prepared move, caret motion retains it.
Raw/control-containing captured destinations remain bytes until complete explicit
replacement; display escapes/truncates and cannot be parsed back into a path.
See DESTINATION-EDITOR.md/VALIDATION.md for scoped evidence. General conflict
skip/replace/rename/journal and multi-origin/EXDEV tasks are still open; this is
no completion of O01/O03. Resource/startup freeze and preview pause unchanged.


## 2026-10-31 — Explicit Copy Skip Existing

Copy default Stop retained; per-dialog Nerd/F4 Skip continues independent roots
without overwrite/merge or applying to Move. Kernel publication collision may
skip only after clean identity-aware staging cleanup; other errors/rescue stop.
Complete skipped/published counters and shared bounded requested-path details,
unchanged4MiB queue/2MiB ACK headroom.806Rust+Clippy/release/fmt; final8FB owned
scene scopedPASS, initial compact440 visualFAIL preserved and corrected. See
COPY-CONFLICTS.md/VALIDATION.md. O03 remains partial: replace/rename, interactive
per-root decisions, Move, durable multi-root recovery and native/global-resource
acceptance remain; restROADMAP/SFTP/shaders/i18n/freeze/preview pause unchanged.


2026-10-31 Copy destination names: per-root F2 editor/Alt arrows/Ctrl+L and same-folder duplication delivered with scoped9FB/818tests. Raw sources immutable; actual selected FD verified before copy. O03 remains partial: general replacement/Move naming/durable recovery pending; resource/startup freeze and previews pause unchanged. See COPY-NAMES.md.

2026-10-31 Folder-size manager order: button/S Queue→Largest→Name, complete measurements only (unknowns last), selected identity and queue order preserved.820Rust/17FB scoped PASS; SIZE-MANAGER.md. Resource/startup freeze and native/global acceptance remain open.

2026-10-31 Search incremental WIP: borrowed scanner prefix/first-hit and bounded core Candidates/Batch32/500/256KiB/exactfinal reconciliation validated (830workspace+6codec tests,Clippy). NOT yet protocol/UI/live-result acceptance; S01 remains partial. Installed omnibar checkpoint unchanged; freeze/previewsPAUSA. See SEARCH-BATCHES.md.

### 2026-10-31 — Copy Keep Both (scoped)

Copy F4 now cycles Stop/Skip/Conservar ambos, reset per dialog; Move unchanged.
Numeric no-clobber publication reuses staged payload after late collisions,
reports actual paths and complete renamed totals. Raw names/compound extensions/
same-folder duplicate,1000 candidates/name255/path4096 bounded.853Rust269/85/499
(+3ignored), Clippy/release/fmt PASS. keep-both-ui1793436602618559696 PASS7FB/
root0ACK; conflicts1793436615203947340 PASS8FB/root0ACK. Internal handlers and
real framebuffer only. Original harness FAIL retained. COPY-KEEP-BOTH documents
limits; overwrite/merge/Move conflicts/durable general recovery remain open.
Resource/startup NOT_MEASURED freeze; previews paused; no global PASS.

### 2026-10-31 — Sizes manager → folder navigation

Alt+Enter and Nerd folder button open selected size row through existing browser/
history; stamped context+selectedID, no UI FS probe/new scanner/cache. F1 updated.
855Rust269/85/501+3ignored/Clippy/release/fmt PASS. sizes-open1793437301816876363
PASS5realFB1000×650/440²(all5inspected),32B largest keyboard/raw0xFF mouse/history/
stale disabled/source hashes/root0drainACK. Original harness menu-route FAIL
preserved. No native input/helper-lifetime/resource/global acceptance; freeze and
preview pause intact. SIZE-MANAGER documents ordinary path replacement semantics.

### 2026-10-31 — Search display order

F7/Nerd Ruta→Nombre natural→Tipo, selectionphysical≠displayposition; samepath
kept on sort andprovisional→final, arrows/HomeEndPage followvisualindices. Cache
invalidated onpopulation evenaddressreuse; no report/pathclone/helper/stat/cache.
858Rust269/85/504+3ignored/Clippy/release/fmtPASS. search-order1793437888618938167
PASS5realFB(all5inspected)/42hits/raw440/root0ACK/1loggedhelpergone strictZno;
batchesregression1793437899122910460PASS7FB/6gone/root0ACK. Originalprepaint
harnessFAILpreserved+sidecar. SEARCH-ORDER records transientnaturalkeys/cost;
reverse/size/date/relevance/paging-index/native/resourceglobalpending. Freeze and
previewpause/historicalFAILs intact.

### 2026-10-31 — Real SFTP/FUSE matrix, compatibility FAIL

SFTP-NATIVE-MATRIX: SSHFS3.7.6 private build (no globalinstall/vendorlink), private
loopbacksshd/internal-sftp/fixturekeys+knownhost; realkernelmount/read/raw/relative
link/externalcat/write/busy-unmount/ordinaryunmount→absent/SSHFS0 PASS. Final
1793442486810260624:6loggedPIDstartgone strictZno/0identityerrors, copyEngineFAIL
EINVAL/0published/cleanstage/sourceintact; directrenameNOREPLACEerrno22; exclusive
links rejectoccupants butdifferentvirtualinodes. OverallFAIL/exit1, originals
retained+classification sidecar. No defaultcopyproof weakened. Need remotevisible-
exclusivepublisher withpartial/rescue semantics beforeworker/UI mountactivation;
notimplementedyet. Installed86bf3ff/f0df843preparationunchanged; no globalN01PASS/
input/nativeGUI/resource/startup/wholelifetimes/daemon/offlineproof. Freeze intact.

### 2026-10-31 — Explicit visible SFTP copy adapter

Core VisibleExclusive opt-in retains partial destinations and reports acknowledged,
completed and uncertain roots; no rollback/source deletion or default-policy change.
Six new tests cover tree/link omission, cancellation, root substitution, numbered
collision and child collision. Native SSHFS report1793444281194564656:
visible_adapter PASS; default stagedcopy FAIL errno22 and overallFAIL/exit1 retained.
No GUI/mountworker/install, native input or resource/startup claim. See
SFTP-NATIVE-MATRIX for scope and explicit omission of unsafe symlink timestamps.

### 2026-10-31 — Mount transaction foundation, not Connect

Pure sftp_lifecycle single-flight ticket/immutable intent, admitted spawn,
late-cancel mount retained, foreground exit reconciled, unreadable table uncertain,
exact-identity explicit disconnect and absence-before-next-transaction. No process
worker/mountpoint provenance/restart journal/UI activation yet. Review retains
port+probe and recheck revokes it; presentation shows port. 885 workspace tests
PASS +5 final controller tests, Clippy/fmt; no new release/install/GUI/resource
acceptance. Full SFTP-MOUNT-CONTRACT and ROADMAP remain open.

### 2026-10-31 — Mountpoint FD provenance, SFTP still open

Owned0700 parent and exclusive random point pinned/revalidated, mount coverage
blocks child inspection before IO, no auto-delete/chmod. Core5newtests plus
realSSHFS heldFD/unmount verification PASS; 890workspaceRust/Clippy/fmtPASS.
Not restart journal/actualworker/ConnectUI/hostilewriter guarantee. Installed
preparation unchanged; next wire process admission/lifetime and visible state.
Freeze and new-preview pause remain; complete ROADMAP remains active.

### 2026-10-31 — Native SFTP process engine

Connection core now executes fixed SSHFS argv and ordinary unmount, with bounded
sealed diagnostics, owned process reap, cancel/retained state and remote root
serving proof. Native full/owner-lifetime/strict-hostkey cases PASS scoped; source
namespace-onlyReady flaw found/fixed, raw failures preserved.895Rust/Clippy/release/
fmtPASS. No UI activation/new install: durable registration/lock/restart recovery,
AuxIo admission/close ACK and controls are next. OS mount survives graceful lab
owner exit and serves a freshly created remote file, not yet GUI-close proof.
N01/resource/global gates remain open; freeze/preview pause/full ROADMAP unchanged.

### 2026-10-31 — Durable SFTP registry and cross-process observation

Private anchored single-transaction registry16KiB, lifetime advisory lock/read-only
second opener, raw paths+Target/port+provenance+generation+PID/start+mount signature.
Atomic publication distinguishes Unsynced/Uncertain; retry only syncs. Unknown/
invalid/future/FIFO/symlink/changed records preserved. Active identities retained;
Absent publish requires fresh own-namespace absence+original point+process reap.
908Rust312/85/511+3ignored,13registrytests/Clippy/release/fmtPASS; syncEIO injected,
not powerloss. Native1793450438576487018 registry observation PASS: owner alive
readonly→owner gone exact mount/process match, fresh server file readable; after
LAB unmount reader reportsNoMount/Missing without rewriting history.7loggedIDs
gone/0errors/survivors, no lifetime coverage. DefaultcopyFAIL/overallFAIL retained.
Still NO AuxIo/actual-window close/recovered disconnect controls/Connect UI/new
install/resources/startup gates. Full ROADMAP active; freeze/previewsPAUSA intact.

### 2026-10-31 — Explicit recovered SFTP disconnect

New core controller never signals/waits original SSHFS or adopts a Child. Current
registry/preimage/lease/privateparent/fullmount revalidated, durable intent before
ordinary unmount child, fresh check after sync; no force/lazy/delete. Read-only,
stale, live/zombie prior unmount and unknown PID-gap reject before child. Busy
retained with diagnostic; retry/completion preserve identity and reap guards.
No-op closed reconciliation avoids another journal write.913Rust317/85/511+3ignored/
5newtests/Clippy/release/fmtPASS. Native1793453782020109493 readonly denial +
new-owner busy→retry→durableAbsent PASS,14loggedgone/0errors/survivors; first
1793453058010388848 preserved. Pythonprivate reaper handled originalserver, not
controller; not PID1/GUI/crashgap/lifetime/inputSO/resourceproof. DefaultcopyFAIL/
overallFAIL retained. Still no shared executor/UI/install. Freeze/previews and
complete goal/ROADMAP remain active.


### 2026-10-31 — SFTP product executor and controls

Core Connection/Store/recovered controller now wired to a lazy fixed transport
worker with bounded1request/1reply and single flight, separate from AuxIo saves.
Explicit reviewed connect; fresh/recovered serving-proof Open; ordinary disconnect,
reconcile and sync buttons/keyboard; polling/reaping while modal hidden. Close
revokes future work and awaits mutation ACK, without claiming blocked readonly I/O
was drained. Registered OS mount survives window exit. Failed publication retains
child/registry and blocks automatic replay; known Absent allows another connection.
Inspection does not hold a writer lease. Missing host sshfs remains visible and no
automatic global install is performed. One registry slot, no connection catalog.

See SFTP-PREPARATION.md and evidence/sftp-native/ui-validation for current logs,
scoped native UI evidence and independent install verification. Native proof uses
actual loopback SSHFS/FUSE with private keys/known_hosts/config and product handlers;
no OS input or resources/startup experiment. Existing default staged-copy FAIL is
preserved; visible-exclusive copy policy needs an explicit product review before
wiring. N01/full ROADMAP and hard/ideal resource goals remain open; previews paused.

Validation:919Rust318core/85helper/516UI(+3existingignored), Clippyworkspace/alltargets-Dwarnings, release/fmt PASS. Native `sftp-loopback-1793455941407692094.json` has UI+freshConnect+mount PASS with8PNG1000×650/3root0;25loggedPID/start identities gone,0readerrors/survivors, not lifetimecoverage. Fixture parent reaped the old SSHFS, product only its own children. DefaultcopyFAIL/overallFAILexit1 are unchanged. Initial recovered UI mismatch1793455368964965907 and its sidecar preserve wrong Registered-vs-Retained expectation; no broadening to arbitrary states. Final negative-tool/440² scene1793455971563736333 has6PNG, no registry creation/source mutation/root0. No native input, clipboard or PSS/Swap/CPU/startup measurements. Long diagnostic lines can still clip; a full copyable transport report remains pending.

Final lifecycle follow-up:921Rust319core/85helper/517UI(+3ignored) and Clippy PASS. Closing increments a shared admission epoch; cancelling close resumes only NEW epoch requests, never revives a queued revoked command. Added deterministic test. Registry Lease now explicitly unlocks only in its acquiring PID, including construction-error paths: deterministic forked-FD test proves child cannot unlock parent, and owner Drop allows a new opener while child retains inherited FD. The prior concurrent new_owner.writable() failure remains in ui-validation/sftp-executor-close-epoch-tests.log; which child caused that historical failure is UNKNOWN. No serialization/relaxed assertions/rerun-only resolution. Current evidence references are listed in ui-validation/checks.json.

Final release UI acceptance: native `sftp-loopback-1793456384121839709.json`, product controls/freshConnect/mount PASS8PNG/3root0; negative-capability and440² `sftp-preparation-ui-1793456409825960118.json` PASS6PNG/root0.25loggedPID/start identities gone/0errors/survivors; not whole lifetimes. Native overall/defaultcopy FAIL remain. Source/log/binary hashes and scopes in ui-validation/checks.json; supersedes previous display-only scene1793455941407692094 for installation. All original reports retained.

Installed source1754f83 through `evidence/0.0.4/installed-checkpoint-1793456472288022118.json`;5installed+5backups independently rehashed exact. Appab1d438d58784c133eeb7bb7c30641bf75c5c0012996121b012a7fc34ef4d911/helper2a177cc39814f29aeb3e7c73cfdf850936debf990499a8a2955d14a5cbbacea8/HTML85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4. No sshfs global installation/config/source publication/tag; host capability still reports missing sshfs.


### 2026-10-31 — Explicit SFTP copy publication review

F3/Opciones in copy review exposes independent visible-exclusive publication and
symlink timestamp omission; default stays staged, no automatic fallback. Both
choices shown before enqueue, captured per job, reset in a new dialog; no Move/EXDEV
delete authority. Options blocks source/path/name editing and stale paste receipts.
Core rejects staged symlink timestamp setter on pinned FUSE parent before creating
a link; visible mode already rejects preserved link times. No silent omission.

925Rust319/85/521+3existingignored, Clippyworkspace/alltargets-Dwarnings/release/fmt
PASS. Native1793457917645506527 UI/copy/mount PASS16framebuffers incl440² and
1000×560;4copiedroots(raw0xFF/tree/link), bytes/rwx/mtime, outside-link-target mtime
unchanged; review/paused no mutation, default staged expectedFAIL, both unsafe link
date choices reject, collision preserves occupant, active cancel leaves57partial
files and original3000files intact.24loggedPID/start identities gone/0errors/survivors,
not lifetimes;3roots exit0 after normal callback. Native defaultcopy/overallFAIL
exit1 retained. Final copy-names UI1793458033930156264 regression PASS. First native
7503490291773 functional PASS but minimum warning visualFAIL preserved+sidecar;
explicit bounded lines fixed and finalframebuffer inspected. No source delete,
automatic removal of visible partials, powerloss/nativeinput/resources/startup claim.
See COPY-PUBLICATION.md and evidence/sftp-native/copy-policy-validation/checks.json.
N01/O01/O03 and full goal remain partial; freeze/new-preview pause intact.

Installed source `e58f576`, report `evidence/0.0.4/installed-checkpoint-1793458216633312563.json`;5installed+5backups independently SHA256 exact. Appfee00a4d1260bdd2163dd84798eafc6e1d3f3626d3de7feb86a812ecd53e611c/helper2a177cc39814f29aeb3e7c73cfdf850936debf990499a8a2955d14a5cbbacea8/HTML85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4. Fixture rwx+mtime equality checked specifically a.txt with whole-second time; byte hashes also cover tree child/raw file, literal symlink payload and outside-target mtime. No nanosecond/ACL/xattr/ownership fidelity claim. No source push/tag/global sshfs install.


### 2026-10-31 — Captured local search calendar

F8/Nerd calendar: Today/Yesterday/today and the six previous days, process-local calendar resolved
once on existing AuxIo with reference captured on choice. Dates populate fixed
UTC bounds and require explicit Search. Date-only UTC syntax unchanged; strict
YYYY-MM-DDTHH:MM:SSZ extension, mixed bounds compare instants. Latest/epoch/close/
reopen/edit guards; readonly ACK separate from helper reap, no per-click thread.
Content editor preserved after ACK, stale paste and overflow reject without
changing criteria. No per-result DU or added metadata calls beyond existing lazy
metadata predicates. See SEARCH-CALENDAR.md for DST/error and platform scope.

937Rust324core/85helper/528UI(+3ignored), Clippyworkspace/alltargets-Dwarnings,release/
fmt PASS. Eight private TZ process cases: leap,23/25h, Santiago midnight gap/
25h yesterday, Havana fold, Apia skipped date,7dates/DST. UI
search-calendar-ui1793460920785323202 PASS26framebuffers incl440² and1000×560,
actual AuxIo+files-helper: inclusive start/exclusive end+subsecond/raw name,
metadata_reads7 for seven candidates, no automatic helper on preset, yesterday/
seven, dates+content/raw filename/editor, equal mixed bounds reject and existing
UTC controls regression. Root0 after callback/ACK,16loggedPID/start helpers gone/
reused strictZfalse, not lifetimes/helpervivoatclose/kernelD/OSinput/clipboard/
otherOS/resources/startup proof. Earlier name-only scene1793460239093706643 retained;
syntax FAIL wiring log preserved. Full goal/ROADMAP, freeze and previewsPAUSA open.

Installed source36b8a18 through `evidence/0.0.4/installed-checkpoint-1793461261514927889.json`;5installed+5backups independently SHA256 exact. App7654450b01fadcfbf004461820ecdcb12f678a07085a05d3a7834dd4f2e9134a/helper59187351e32c01e56925bbb4fc70da053151bd9c585b28c23426c1216790a6dd/HTML85ea1b8009efb3f221fcb1b639701ec3a50aeb72a8b1ddf8783091d08da403a4. Source publication/tag/global timezone/config changes not performed.

### 2026-10-31 — Size manager captured report and subtotal

SIZE-MANAGER now documents a completed-only checked apparent subtotal and a
bounded64KiB report: exact bytes/observation timestamps/raw debug escaping,
unknown/error/cancel/progress distinct from zero, explicit F5 refresh, keyboard
and mouse scroll/copy-request/return. Cached UTF8 byte-range layout (4096lines)
reuses text; no new scanner/worker/dependency/disk cache. Stale contexts block
Copy/Refresh and expose explicit Recapture. OS clipboard delivery remains
UNVALIDATED; local caps are not a resource or speed proof. Sizes across different
observation times are not a filesystem snapshot, deduplicated usage, freeable
space or operations authority. Full ROADMAP remains active; native input,
resource/startup gates, SFTP profiles/diagnostics, broader operations/shaders/i18n
and historical failures remain open. New previews remain paused.

### 2026-10-31 — Search locations in background tabs

SEARCH-BACKGROUND-TABS: Nerd result button/CtrlEnter creates a metadata-only tab
for directory or file parent+raw focus without closing search or changing active
pane/tab/root/generation. Existing8tabs/3MiB joint stored+closed+other-pane budget,
empty-history charge, no new listing/helper/prefetch until ordinary activation.
945Rust/Clippyworkspace-Dwarnings/release/fmt PASS; actual search_batches scene
1793463564614078345 scopedPASS10FB/provisional-retained/raw440/emptydir/8tablimit/
sourcehashes/root0ACK/3loggedPIDstartgone strictZfalse. Source/test accounting
reserve issue fixed, originalFAIL preserved. Native input/resources/lifetimes and
wholeROADMAP remain open, freeze/newpreviewPAUSA intact.

### 2026-10-31 — Search raw-path multi-selection and atomic location batches

SEARCH-MARKS: Nerd checkbox/Ctrlclick/Shift range/Space/Insert/CtrlA-D and mouse
Todo/Ninguno; focus independent of marks, visual-order ranges,128paths/64KiB
borrowed preflight. Raw paths reconciled provisional→final with pruned count;
explicit empty never falls back. Monotonic result epochs reject old row hits.
Marked locations open as atomic metadata-only tab batches in visual order under
existing8tab/jointbudget, no preloads.951Rust/Clippy/release/fmtPASS and actual
helper scene1793464566350502738 scopedPASS15FB/root0ACK/3loggedhelpersgone strictZno.
Copy/Move/Trash from multi-origin marks still need Source/target/queue integration;
no clipboard/native-input/resource/globalPASS. Freeze/newpreviewPAUSA intact.

## Search-result Copy/Move — 2026-10-31

F5/Copiar and Shift+F6/Mover from marked search results now capture up to128
roots/32parents through AuxIo, validate effective FD proofs and open the existing
review/queue. Cancel retains marks; stale intent/changed root cannot redirect the
job. Global queue/FD/metadata accounting remains shared. Native multi-parent Move
reports effective observed locations and confirmed/pending/uncertain roots;
EXDEV remains one regular nlink1 only, not batch/tree/hardlink transfer.
964Rust327/85/552 PASS+3existingignored, Clippyalltargets-Dwarnings/release/fmtPASS.
Final scene1793470188336000116 scopedPASS27realFB,5loggedhelpersgone strictZno,
root0ACK; cancellation/paused/replacement/partial conflict/raw/copy5/move5 and
single EXDEV actual bytes/inodes verified. See SEARCH-TRANSFERS.md and
evidence/search-order/transfer-validation/checks.json. Original FAILs retained.
No inputSO/clipboard/lifetime/performance proof. Resource/startup NOT_MEASURED
freeze and newpreviewPAUSA intact; full ROADMAP/global gates remain open.

## Explicit pending Move retry — 2026-10-31

Incomplete multi-parent Move now offers Nerd **Reintentar pendientes / Ctrl+R**
in its own report. Same original token/ledger/destination, fresh ID, no recapture
or replay of Confirmed; Uncertain/unresolved cleanup blocks. Atomic queue
reservation transfer restores full outcome/proof on rejection. Repeated failures
retain one diagnostic, cancellation before execution keeps proof until ACK.
970Rust327/85/558+3ignored PASS, finalClippyalltargets-Dwarnings/release/fmt; full
suite before only title text/assertion, targeted report-title test also PASS.
Scene1793471063548051155 scopedPASS29FB/root0ACK/6loggedhelpersgone strictZno,
occupied retry then fixture occupant removal→CtrlR pending-only/sameinode/1root.
Previous command-prefix FAIL and wrong success-title capture preserved/sidecars.
See SEARCH-TRANSFERS.md and evidence/search-order/retry-validation/checks.json.
No nativeinput/clipboard/lifetimes/metrics. PSS/Swap/CPU/startup NOT_MEASUREDfreeze;
new previews paused, full ROADMAP and global acceptance remain open.

## Keyboard and mouse operation reports — 2026-10-31

Up/Down/PgUp/PgDn/Home/End and wheel share bounded, viewport-derived navigation.
Nerd first/previous/next/last buttons have no active target at unavailable edges;
line range/total and key hints stay visible. Paint measures/draws full visible
rows only, resize clamps a full last page, pan clamps actual visible overflow.
No report clones/newworker/decoder/dependency; no PSS/speed claim.975Rust327/85/563
(+3ignored)PASS, final2focusedtests/Clippyalltargets-Dwarnings/release/fmtPASS;
fullsuite before onlyteststyle/tooltiplabels. Scene1793471804962465697 scopedPASS
34FB440/1000, report10/7visible→full-last/widefitsall0, operationunchanged,
root0ACK/6loggedhelpersgone strictZno. DisabledEnd fixtureFAIL+sidecar and2Clippy
test-styleFAILs retained. See REPORT-NAVIGATION.md and
evidence/search-order/report-navigation-validation/checks.json.
Freeze resources/startup/newpreviewsPAUSA, nativeinput/clipboard/AT-SPI/lifetimes
and global/fullROADMAP acceptance remain open.

## Saved/reusable search criteria — 2026-10-31

F2/Nerd catalogue and CtrlS named capture; readonly review, raw root, name/literal,
4size/UTCfields, localhidden/editor/order. Load stored/current search root fills
draft only; Enter executes. Metadata ACK never applies search fields and selects
a newly saved entry only for exact unsuperseded request/panel.32entries/128KiB,
2passcodec, regular-onlybounded reads, checkedatomic/CAS entry/creatorPIDflock;
existingAuxIo/sentACK/explicit retry-discard, published-unsynced distinct.
993Rust334/85/574+3ignored/Clippyworkspacealltargets-Dwarnings/release/fmtPASS.
Scene1793475882305119370 scopedPASS4exit0ACK/14FB/6loggedhelpersgone strictZno,
load-nohelper then Enter/raw reopen/rename/remove/futurepreserve/compact440 fields
andcontentrows. Root fields32B and raw4096B/global128KiB are local bounds only.
Inheritedflock risk tested with2privateforks; original WouldBlock exactcause not
claimed. OriginalFAILs+sidecars andfirstPASS before new-row selection fix retained.
See SAVED-SEARCHES.md and evidence/search-order/saved-search-validation/checks.json.
No inputSO/OSclipboard/AT-SPI/lifetimes/powerloss/kernelD/resource/startup/global
acceptance; freeze/newpreviewPAUSA/fullROADMAP intact.

2026-10-31 Folder-size manager F5/Nerd Recalculate requeues only the selected completed observation with fresh ID, preserves other totals/pause/admission order, removes old subtotal/sort value immediately.997Rust+focused sort/Clippy/release/fmtPASS;24FB/rawhelper scopedPASS, SIZE-MANAGER.md. Resource/startup freeze and global/native gates remain open.

2026-10-31 Search elapsed windows: F8/key4–6 or Nerd1/24/168h choices materialize fixed UTC seconds on existing AuxIo; calendar1–3 remains local dates.999Rust/Clippy/release/fmt and23FB/helper scopedPASS; SEARCH-CALENDAR. Natural-language age syntax/custom rolling periods/index/pagination/native/resources/full ROADMAP remain open.

2026-10-31 Search pagination FOUNDATION_ONLY: retained FD walk/search_pages helper with exact bounded credit,500/256KiB windows and cumulative limits, core request-bound ledger.1007Rust/Clippy/fmt/dev-helper and7actual pipe cases PASS; SEARCH-PAGES.md. UI reader/history/navigation/progress/close and installation pending; installed SearchBatches still capped500. No resource/startup/global/native acceptance.

2026-10-31 Search pagination UI: F9 retained walk, CtrlPgUp/Dn/Nerd arrows, up to8pages/2MiB accounted history, explicit eviction/page-local marks, empty terminal/Stop preserve accepted data; invalid protocol cannot resurrect rows.1023Rust/Clippy/release/fmt, six Apps18FB/eightloggedhelpersgone scopedPASS; SEARCH-PAGES. Exact raw-path capacity bug red→green; seven same-SHA release pipe cases reused. No resource/startup/native/global PASS; index/global order/arbitrary history remain open.

### 2026-10-31 — Local SFTP connection profiles

SFTP-PROFILES: F2/Nerd catalogue, keyboard/wheel/mouse, named save/CtrlS, explicit
load-only draft/invalidated Review, remove exact entry, retry/discard/sync-only ACK.
32rows/128KiB, borrowed preflight/Target validation,0600/regular-only/atomic writer
with interprocess lock; same AuxIo, no new worker/credentials/automatic connection.
Modal+catalogue revision+stable ID protect stale load/remove even after ID reuse.
1040Rust349/91/600+3ignored PASS; final Clippy/release/fmt PASS.
Scene1793484790015975923 scopedPASS3apps/7realFB,440caption fits, no realhost/inputSO
or metrics/lifetimes/activefsync-close proof. Original clipped subtitle positive
and failures preserved; old spawn-marker test cause OPEN, strengthened assertion
without runtime supervisor change or serialization. Mount registry still one slot,
profiles are not host trust/keyring/reconnect/multi-mount. N01/N02/fullROADMAP open;
freeze resources/startup and preview pause/historical FAILs unchanged.

### 2026-10-31 — Readable SFTP diagnostic capture

SFTP-REPORT: F4/Nerd, captured draft≠observed connection, retained diagnostic,
24KiB/explicit truncation/control escapes/raw mountpoint representation. Glyph
width/whitespace wrap with scalar long-token fallback, borrowed line ranges/cache,
height clamp/source anchor on resize, arrows/wheel/pages/HomeEnd and Nerd controls.
F5 only recaptures held state; no probe or network dispatch. Copy requests exact
current snapshot; stamp/reportID protect stale refresh/back/copy/nav. Report and
profiles reject hidden form transport controls; readonly report rejects paste.
1050Rust349/91/610+3ignored/Clippyworkspace-Dwarnings/release/fmt PASS. Final report
1793486210147321426 scopedPASS2apps7PNG/root0ACK; profiles regression1793486235803344634
PASS3apps7PNG sameSHA. Copy handler observed, OS delivery UNVALIDATED; no actual
host/network/inputSO/lifetimes/activehelperclose/resources/startup experiment.
Original word-splitting positive+sidecar and fixture/compile/count failures retained.
No new worker/dependency/previews. Full ROADMAP/N01/N02 and historical gates open.

### 2026-10-31 — Archive operations shared foundation (not UI acceptance)

ARCHIVE-OPERATIONS-CONTRACT: common ZIP32 member streaming stored/DEFLATE,
realStreamEnd/exactconsumption/CRC, localneeded-version/header/extra/descriptor
checks,8KiBinput/32KiBoutput, quota-before-sink and cancellable reads inclEINTR.
Office adapter preserves1MiBVec/8MiBcompressed caps without duplicate decoder.
Read-only complete plan checks raw paths/types/prefix/canonical collisions,
64depth/255Bcomponent/20000nodes/1GiBoutput, local-range overlaps; no destination IO.
1068Rust367/91/610+3ignored, Clippyworkspace-Dwarnings/release/fmt PASS;30release
helperwire Python-generated stored/DEFLATE/descriptors PASS. No new dependency
package/version/checksum, only core edges to existing permissive codec packages.
No extraction/creation execution, queue/UI or new installation; current appCFAA
checkpoint5hashes independently unchanged. Initial false premise about legacy
emptyDEFLATE acceptance disproven and red retained, no vulnerability claim.
Next FD-bound staging/publication/rescue+sharedadmission+reviewUI; ZIP64/TAR/links/
otherformats and fullZ01 remain open. Freeze/newpreviewpause/globalFAILs intact.


### 2026-10-31 — ZIP extraction review and shared queue

Actions/context/catalogue extraer/zip → existing modal; source filename first,
new child destination editable/raw-preserving, review readonly then explicit Apply.
AuxIo+global operation worker only; no new thread/decoder/dependency.4MiB controller
unchanged, outcome must fit fixed2MiB report slot,3prepared+12transientFD reserve;
large graphs reject before mutation. Result drops before next dispatch; failed ZIP
cannot strand input charge.1093Rust387/91/615+3existingignored/Clippy/release/fmtPASS;
scene1793491842976117966 scopedPASS7PNGs, bytes/modes/raw/unchanged ZIP/other-tab
selection, occupied/traversal/CRC, callback paused→ACK→root0. Original positive
scene missing source basename at440 preserved+visualFAIL sidecar; final corrected.
Dedicated cleanup/sync retry buttons/durable catalogue, larger plans, create/ZIP64/
TAR/links/metadata/otherOS and fullgoal remain. No resources/startup/nativeinput/
clipboard-receiver/activekernelD/powerloss proof. Freeze/previewsPAUSA/oldFAILs stay.


### 2026-10-31 — Direct count is not a recursive size

Groups cards no longer label first-level stat sums as folder sizes. Typed direct
counts have no byte field,1024entry cap,complete/limited/partial/unavailable;
ReadDir only, no child name/type/stat requests. Existing root Stats supplies date.
Explicit calculator/helper unchanged, `Total:` only after measured recursive
completion; zero differs from unknown. Shared compare_sizes for core/active/
inactive browser, plain directory stat length never a total, unknowns last within their group both
ways. Same-root measured data used in inactive sorting; late count replies kept
across focus switch, avoiding permanent contando. Rebuild on Size invalidation
only, no extra Name/Modified sort on every switch. No new worker/cache/budget.
1100Rust388/91/621+3ignored/Clippy/release/fmtPASS; scene1793493877395472428 scopedPASS
4PNG1000/440,3directvs8B,0Bvsunknown,1024limit,bothorders,2loggedhelpersgone/root0ACK.
Original ETXTBSY fixtureFAIL+poison and wrong ascending-first GUI-harness FAIL+2PNG/
sidecar preserved. Inspector no-follow policy/shortcut now fit separately.
No resource/startup/native-input/lifetime/allocateddisk/snapshot/globalPASS claim;
freeze/newpreviewsPAUSA/fullROADMAP remain. Installation recorded separately.

Folder direct-summary/sort consistency INSTALLED7983c0f/app43a09343ee99,
record1793494152752973446;5installed+5backup hashes PASS.

### 2026-10-31 — Retire size observations without refreshing browser

SIZE-MANAGER installed8347b65/app18cb22c21e8c. O/Info button removes terminal row
only after active ACK; report/ShiftO uses a frozen completed-ID set, no newer values,
errors/pending lost, no filesystem deletion or refresh. Fresh IDs/control stamps,
128slot recovery unit proof, both-pane size-sort invalidation and browser selection
preserved.1128Rust+Clippy/release/fmtPASS;1793501340564075091 scopedPASS4PNG1000/440,
actual4DUhelpers then no helper on forget/pausedreadmit; whole own fixture tree
unchanged. Records forgotten are unknown, never measured0B. Freeze/native/global
acceptance unchanged; propagation of ordinary live size updates outside these
explicit actions remains a separate audit.

- 2026-11-01: live dual-pane folder-size sorting: accepted measurement updates
  inactive Size order while active Name order stays stable; browser and manager
  recalculation remove stale keys in both panes.1131 workspace tests/Clippy PASS.
  Real-helper scene and installation tracked in VALIDATION, no global gate claim.

- 2026-11-01: consistent Name/Size/Modified sort header with visible criterion/
  direction in single and dual lists; compact Nerd controls and keyboard palette
  date command. Inactive sorting preserves owner/measurements, rejects stale
  revisions.1133Rust +3existingignored/Clippy PASS; functional scene/install below.

- 2026-11-01: explicit zero selection survives local filter removal/Escape, with
  no default copy source; per-pane edit/clear controls and visible/loaded counters
  for mouse/keyboard workflows.1138Rust +3ignored/Clippy PASS. Full goal open.

- 2026-11-01: Terminal here (U02 partial) in Actions/context menu/palette: current path
  captured, uwsm service broker + helper ASCII hex→raw cwd→xdg-terminal-exec;
  same open slot, no shell text/default change/own daemon. Other platforms
  pending; acceptance/installation are recorded in TERMINAL-HERE and VALIDATION.

## Capture audit follow-up (NODE)

Fixture captures publish PPM atomically and optionally use request IDs, ACKs and
explicit observed/repainted comparisons. 1210Rust/Clippy/release/fmt and paired
X11×2/Wayland observations passed; INSTALADOe9281b5/app9e01f8493b57,5backups+5hashes,installed21captures/2rootexit0/6helpersgone,ROIequal. See `CAPTURE-AUDIT-CONTRACT.md`.
The original X11 missing-name frame remains an unresolved rendering/capture
observation. Matching ROI hashes in these samples do not establish its cause or
absence, and no resource/startup experiment or native-input claim is included.

## Search and bookmark path labels (NODE)

Result basenames/parents and bookmark path/history captions use visible raw-byte
and control escapes while retaining original destinations. History's caption
filter matches the displayed escapes; filesystem search syntax is unchanged.
1212Rust/Clippy/release/fmt and X11+Wayland50captures/6drainedroots/14tracked
reader identities gone/reused passed. INSTALADOcb74513/appff8b0c38b940,
5backups+5hashes, installed25captures/3roots/7trackedreadersgone. Original X11 capture
anomaly and remaining path surfaces/full-name clipping remain open.


- NODE 2026-11-01: properties shows the full requested/verified paths and raw target
  in scrollable lines; nine permissions and three actions fit at 440×440.
  X11/Wayland functional PASS 31 captures each and the rejection/ACK regression PASS.
  See FILE-PROPERTIES.md and evidence/properties-layout. Copy report, the remaining
  metadata, OS input/AT-SPI/2× and the rest of the complete goal are still pending.


- NODE 2026-11-01: common escaped labels in copy/move/ZIP and the rename
  review; the copy original distinguishes a raw byte from a literal escape without touching
  editors/targets. Copy X11/Wayland PASS 13 captures each, rename raw-parent
  PASS. Clipping of headers/columns and selecting/copying paths are still pending.


- NODE 2026-11-01: S02 regex foundation SOURCE ONLY, core::search_regex bounded
  first-span/line including zero-width/EOF/multiline,429core+3ignored/focused4PASS.
  Existing std-only engine unchanged. CONTENT-REGEX-CONTRACT.md tracks required
  wire/snippet/page/helper/UI work; regex is NOT exposed/installed/complete yet.


- NODE 2026-11-01: regex wire foundation SOURCE ONLY: mode-bound optional
  regex_end spans, empty/EOF positions and no fabricated zero-width snippet;
  report/batch/page/stream validators preserve literal strictness/default mode.
 1224workspace433/95/696 PASS+3ignored/Clippy/fmt;500-hit wire limits unchanged.
  Query/helper/UI still pending in CONTENT-REGEX-CONTRACT; no new installation.


- NODE 2026-11-01: S02 advanced `regex:` content query integrated through core,
  checked-FD helper, batch/page streams and UI ∅ marker. Full allowedUTF8 file,
  multiline/empty/EOF; literalKMP preserved, bounded compiled state/read budget.
 1228Rust434/98/696+3ignored/Clippy/fmt/release and10CLI PASS; X11/Wayland28FB
  each/23loggedhelpersgone/root0. See CONTENT-REGEX-CONTRACT.md for exact receipts,
  initial diagnostic-build FAIL and corrected F9 fixture scope. Guidedregexmode,
  Unicodecasefold/otherencodings/index/remainingfullgoal stay pending.


- NODE 2026-11-01: guided Literal/Regex content selector and CtrlF6 preserve raw
  text/name/facets, no auto search, stale paste/result controls revoked. Presets
  v1 literal retained; mixedregexv2 mode explicit, real Save/Reload/Load tested.
 1230Rust435/98/697+3ignored/Clippy/fmt/releasePASS; X11/Wayland37captures each,
 27loggedhelpersgone/root0 and440px nonoverlap. GUIDED-REGEX-CONTRACT.md records
  limits and the preserved pre-paint fixture FAIL. Whole goal/global gates remain.


- NODE 2026-11-01: marked-folder size batch via command catalog and managerM,
  only visible marked dirs/no focus fallback/files/links; preflight/busy preserve
  old queue+pause, exact raw names, existing helper/ACK.1232Rust435/98/699+3ignored,
  Clippy/fmt/releasePASS. X11/Wayland10captures each/19loggedDUhelpersgone/root0,
 17B exact selected folders,440positive nonoverlap controls. See
  MARKED-FOLDER-SIZES.md; apparent per-name semantics/physical metrics remain distinct.


- NODE 2026-11-01: marked-only captured size report/scope+ShiftI, queue-wide default
  retained, missing observations explicit/not0, measured-only checked subtotal;
  Forget binds only captured IDs and preserves unselected observations. No new
  scan/helper.1235Rust435/98/702+3ignored/Clippy/fmt/releasePASS; X11/Wayland15FB
  each/19loggedDUhelpersgone/root0,60Bqueue/17Bmarked/43Bretained,440controlsfit.
  MARKED-SIZE-REPORT.md records limits; fullgoal/global gates stay active.

- NODE 2026-11-01: optional allocation foundation SOURCE ONLY in shared DU walker,
  regular-file blocks by name/device-inode with65,536identity bound and explicit
  overflow/limit/changed unavailability.1242Rust435/105/702+3ignored and strengthened
  helper105/Clippy/fmtPASS. No wire/UI/install change; ALLOCATION-CONTRACT.md
  defines semantics and integration pending; fullgoal/resource freeze remain.

- NODE 2026-11-01: allocation wire/core reader SOURCE ACCEPTED, opt-in v1
  measure_directory_allocated + typed final-only metrics; legacy usage unchanged.
  4096B reader/ID/mode/monotonic/exact final/sticky failure validation.1249Rust
  (442/105/702)+3ignored/Clippy/rustfmt/releasehelperPASS;7CLI incl65,536identity
  boundary, alias-at-cap/raw/sparse/legacy. UI job/queue/report integration pending;
  no install,5installedhashes unchanged; fullgoal/resourcefreeze remain active.

- NODE 2026-11-01: typed allocation worker+per-attempt queue SOURCE ACCEPTED;
  Job defaulttuple preview retained, tracked Measurement/mode-bound finals,
  retry/recalc clear oldallocation+retain mode/newIDs, staleACK/batchatomicity
  unchanged.1256Rust443/105/708+3ignored/Clippy/rustfmt/releasePASS;7CLI plus
  legacy marked-report regression X11+Wayland15FB/19helpersgone/root0 each.
  Historical folder-view WouldBlock initialFAIL preserved/nofixclaim. Noinstall;
  explicit allocation actions and report/UI presentation next; fullgoal/freeze.

- NODE 2026-11-01: allocation product controls/catalog focused+marked, manager
  B/ShiftB+9footer controls in3rows, per-row B mode/details and immutable reports
  expose regular-file blocks pername/peridentity, explicit unavailable/absence
  and no cross-root union/freeable claim.1258Rust/Clippy/fmt/release+7CLIPASS;
  actual allocated UI Wayland/X11 each22FB/2root0/25helpersgone. Local install
  acceptance follows; fullgoal/resourcefreeze/historicalfolderview+X11 remain.

- NODE allocation UI source8606261 INSTALLED checkpoint1793539706031130409;
  installed1793539716211647444 PASS22FB/2root0drain/25helpersgone,5+5hashesverified.
  Appb9941ab1acfb/helpera8fc8abaf26c; ALLOCATION-UI.md covers scope/limits.
  Full original objective and native/resource/global gates remain open.

- NODE 2026-11-01: general listing audit confirms existing notify usage is theme/media
  only. SOURCE listing_watch foundation:2targets,1controlworker,coalesced epoch/serial
  ACKslots,async attach/retry/stop;5local testsPASS/Clippy/fmtPASS. Initial workspace
  1263PASS beforeattachment refinement;latestfull1262PASS+1historical folder-view
  WouldBlockFAIL preserved,no globalPASS. Model bridge+root-alias coverage pending;
  no install/resourceexperiments; LISTING-WATCH-CONTRACT.md states limits.

- NODE 2026-11-01: listing watcher root/ancestor alias repair SOURCE ACCEPTED.
  Reproduced old root-symlink rearmFAIL; background resolved dev/inode checks
  before/after attach and every2s detect persistent alias resolution changes;
  no content scan. Per-arm stale native callbacks rejected.1267Rust+3ignored,
  final9watcher tests/Clippy/fmtPASS. Modelbridge/auto-refresh UI stillpending;
  no install/resourceexperiments; historicalWouldBlock/X11 notfixed/closed.

- NODE 2026-11-01: listing_sync Model bridge SOURCE ACCEPTED:stablebrowser/tab/path
  watches,coalesced deferredrefresh+serialACK,rangeanchor/explicit-empty/rawmarks
  and painted namedviewport restored aftermetadata,dual/tabs/closeACK/resume.
  1272Rust443/105/724+3ignored/Clippy/fmt/releasePASS;watch UI X11/Wayland6FB2roots
  each;allocation regression22FB2roots25helpers each. Columns/SpaceMap defer and
  native/offline/globalgates remainpending; fullgoal/resourcefreeze unchanged.

- NODE listing auto-refresh source81837e5 INSTALLED checkpoint1793544731266983878;
  installedwatch1793544742660384859 PASS6FB/2root0drain + allocation regression
  1793544743043408636 PASS22FB/2root0drain/25helpersgone.5+5hashes app8f7733dfce15.
  Columns/SpaceMap own updates/native input/offline/global/resource gates remain
  open; no globalPASS, previews/resourcefreeze unchanged.

- NODE 2026-11-01 SpaceMap marks de6c3a4/cd12a06 installed record1793555602802600273:
  projected All/Invert/Clear/MCInsert, ignored retained filter, stale context guards,
  partial/unknown/raw/hidden rows.1294Rust+3ignored;finalClippy/fmt/releasePASS;
  sourceWayland/XvfbX11+installed16captures3roots6helpers each. Five backup+install
  hashes; SPACE-MAP-SELECTION.md. Inspector CopyF5 caption still needs context audit;
  fullgoal/native/2x/offline/resource gates and historical X11 anomaly stay open.

- NODE 2026-11-01 command context f9e5388 INSTALLED1793556391638203701:
  inspector/menu copy captions no falseF5, generic copy tooltip, palette contextual
  recalculation label/alias/F5CtrlR; keyboard+MC share f5_action; actualMap !dual
  consistency guarded synthetically (normaldual setsList; no user-visible repro).
  1296Rust447/105/744+3ignored/Clippy/fmt/releasePASS;Wayland/XvfbX11+installed
  each19FB3normal roots7knownhelpers. Five backup+installhashes app5a9660a4ed5c.
  Initial watcher-testFAIL kept: Create+Modify maystraddle refresh; ownedkernel
  audit, one-event rename stimulus and laterwrite assertion; production unchanged.
  Next real command-availability parity Map keyboard/inspector/menu/palette/MC;
  fullgoal and original anomalies/native/2x/offline/resource gates stay pending.

- NODE 2026-11-01 Space actions 6d9c5dd/fixture 924f8d4 INSTALLED
  record 1793558033383070484: shared map availability for MC/palette/menu/
  inspector/dispatch; mark validation before sanitizing without fallback; review
  concurrent with a queued copy, F5 reserved and Ctrl+R Action Refresh; the existing preview
  in the palette, properties of one mark. 1301 Rust 447/105/749 + 3 ignored/Clippy/fmt/release
  PASS. Source Wayland/Xvfb X11 + installed each 29 FB 4 normal exits 15 known helpers.
  A missing XDG-data parent in the fixture caused two FAIL 28 FB; the same binary after mkdir PASS;
  production Trash intact. 5+5 hashes app 2cae3b73db4e. SPACE-MAP-ACTIONS.md.
  Next: review parity with an active queue outside Map; textual non-UTF-8 CopyPath
  still to_string_lossy (no lossless clipboard claim). Full goal/freeze/gates pending.

- NODE 2026-11-01 QueueReview 9ddd253 INSTALLED 1793560331250393795: Copy/
  Move/ZIP review available with an active copy from MC/palette/menu/inspector/Action;
  validate before sanitizing prevents a fallback from filtered/hidden/stale marks;
  a full queue keeps the form; charges/IDs/ACK/admission intact. Progress above
  MC and controls in 2 narrow rows; the last result above the compact notice.
  1306 Rust 447/105/754 + 3 ignored/Clippy/fmt/release PASS. Source Wayland/Xvfb X11 + installed
  each 35 FB 5 normal roots; Map regression 29 FB 4 roots 15 helpers on each backend. The initial
  Xvfb wrapper cleanup FAIL with a PASS receipt is kept; the same binary alone exit 0.
  Five backups+hashes app a715c17069d6. QUEUE-REVIEW.md.
  New pending item observed: Dual 440 + expanded sidebar has NameWidth 0, the control
  at wide 1000 normal; prioritize the name/adaptive metadata and review narrow List.
  The original wide X11 anomaly is not closed. CopyPath non-UTF-8/native/2x/full goal/
  freeze and the rest of the ROADMAP are still pending.

- NODE 2026-11-01 NarrowNames f6a735f/fixture 031aa00 INSTALLED 1793562789004828933:
  name cells first, size/date optional depending on width, without formatting omitted
  data; List/Dual min 440 + sidebar with readable names, three sort targets and raw
  focus/marks/owner intact. 1309 Rust 447/105/757 + 3 ignored/Clippy/fmt/release PASS;
  source Wayland/Xvfb X11 + installed each 27 FB 2 normal roots; queue regression 35 FB 5 roots.
  Initial FAIL expecting 1000×650 vs helper/PNG 1000×560, fixed with the same binary;
  the fixture hex lint fixed without suppress/serialization. 5+5 hashes app 8f88cc804da3.
  NARROW-NAMES.md. Arbitrarily long names can still be clipped; the original wide
  X11 anomaly is not closed. Next: a lossless policy for textual non-UTF-8 CopyPath;
  full goal/native/2x/other OSes/offline/resources and the freeze continue.


## NODE reconciliation — 2026-11-01

The old rows and entries are history, not proof that a pending item is still
open. Sources were checked before choosing C02: guided regex already exists
(`search_ui/state`, GUIDED-REGEX-CONTRACT); TerminalHere already exists (`open_job`,
TERMINAL-HERE); SFTP and operation reports already have navigation/copy
(`sftp_report_ui`, `mutations`, SFTP-REPORT). They are not reimplemented because of an old note.
Not every capability in this matrix has been audited or closed.

| Recent progress | Current evidence | Limit of the claim |
|---|---|---|
| Space actions/MC selection and review during an active copy | SPACE-MAP-ACTIONS.md, QUEUE-REVIEW.md | Availability and own handlers/operations; native/global gates open |
| Names in narrow List/Dual | NARROW-NAMES.md | Name prioritized over metadata; not a causal fix of the historical wide X11 anomaly |
| Copy path and local URI without raw loss | COPY-PATH.md, evidence/copy-path/assessment.json, acceptance 84469ac | Installed 5c258f3a8d21, 1311 tests; an isolated sink is not native clipboard delivery |
| Compare trees C02 | DIRECTORY-COMPARE-CONTRACT.md, core/directory_compare.rs | Foundation only; no helper/UI nor installation of this capability |

The FULL goal, synchronization and every other pending item are kept.
Absolute PSS/Swap/CPU/startup freeze and new previews paused; no new
result of this reconciliation proves metrics, other platforms, OS input,
clipboard, AT-SPI, 2x, complete lifetimes nor the absence of offline blocking.


- C02 NODE: comparison transport/helper and supervisor verified in
  evidence/directory-compare-wire/assessment.json; still without a visible panel nor
  installation. Do not confuse the public transport job with Model admission/close,
  nor its observations with permission to synchronize.


- C02 NODE: visible comparison integrated through directory_compare_ui; Actions /
  palette / background menu, reversible roots and a filter of retained observations.
  Diagnostics without a report do not invent a verdict nor erase the error. The
  admission/close waits for the ACK; Pan keeps the other names readable.
  DIRECTORY-COMPARE-UI.md and its assessment contain the final receipts/installation.
  The size failure of a QueueReview regression with two simultaneous Wayland fixtures
  is retained; the same version isolated PASS, exact cause unconfirmed.
  No causal fix of native anomalies nor global PASS; synchronization stays open.

- C02 NODE: `directory_sync` source-only fresh review preparation adds Update and
  Mirror in either direction plus bidirectional Merge without inferred deletions
  or timestamp winner. Explicit content/link/type/basic-metadata/destination-only
  conflicts, checked one-row choices, subtree coverage and complete-review block
  on partial/changed observations. It consumes a new comparison, never an old
  report, checks canonical root containment and final root identities. This is
  not an execution token or installed synchronization. DIRECTORY-SYNC-CONTRACT.md
  records the audited copy/queue/rename/trash/EXDEV reuse boundaries and required
  supervised visible review, fresh pinned manifest, recoverable replacement and
  removal, journal/barriers, queue/cancel/close ACK and crash/reopen/undo gates.
  Missing-name copies alone do not meet C02. Full goal and resource/preview freeze
  remain active; comparison UI is still the installed checkpoint.

- C02 NODE: synchronization preparation now has the helper
  `prepare_directory_sync` + Intent version 1 and a supervised Job, SOURCE ONLY.
  Mode version/ID/roots/budget, the strict comparison ledger reused,
  proposals/Pending choices derived locally, a 23400576 B stream only for this
  command; the ordinary 4 MiB intact. 1388 Rust 489/112/787 + 3 ignored/Clippy/fmt/helper debug
  PASS, 17 new tests and 18 real helper CLI runs including both full inventories
  of 8192 control-character names per side (>4 MiB) without truncating nor changes in fixtures.
  EOF/exit 0/reap before the result; a non-zero exit/EOF without exit/cancel after terminal
  reject and reap own children. Still without Model/admission/root close ACK/visible UI
  nor Apply/FD manifest/journal/recovery/sync installation. Next: a visible dry run;
  DIRECTORY-SYNC-CONTRACT and evidence/directory-sync-wire document the scope.
  The FULL goal/freezes and the installed comparison remain; no global PASS.

- C02 NODE: `directory_sync_manifest` SOURCE ONLY fresh pinned evidence reuses
  EXDEV inventory without changing FLMOVE/Entry wire format or legacy bounds.
  Parent/root pins, original-path reopen/mount/ancestry checks, per-entry mount
  proof, full revision/content/xattr-metadata digests, namespace and hardlink
  internal/external groups; both8192child inventories plusroots fit. Shared regular
  payload budget includes final rehash, no oldreport/choice-to-Apply conversion.
  1409Rust502/112/795+3ignored/13newtests+Clippy/fmtPASS normal, including real
  independent filesystem and ancestor-alias retarget. Initial1408PASS followed by
  collapsible-if lint retained/fixed. DIRECTORY-SYNC-MANIFEST.md and assessment
  record scope; canonical seal/new approval, helper/UI, journal, recoverable
  effects/queue/crashreopenundo pending. Installed dry-run remains289db327/helper170f.
  Absolute resource/preview freezes, full goal and global gates remain active.

- C02 NODE: `directory_sync_record` SOURCE ONLY canonical sealed evidence and
  byte-exact/rehash live binding; decoded history and56B journal comparison keys
  do not grant operator approval, fresh barriers or writes. MAXwire4657718B,
  complete16386entries beyond4MiB, raw fields/roots/metadata/mounts/ancestors,
  directory child hashes and derived hardlinks validated.1425Rust518/112/795+3ignored
  and16newtests/Clippy/fmt PASS normal. Initial map inference/lint retained; real
  deep-root fixture RED→GREEN separates absolute-root depth from content depth.
  DIRECTORY-SYNC-RECORD.md/assessment record scope. New action-plan/choices seal,
  explicit approval, helper/UI/session, durable journal/recoverable effects/queue/
  crashundo remain pending; installed dry-run unchanged, fullgoal/freezes active.

- C02 NODE: `directory_sync_plan` SOURCE ONLY derives all five modes from canonical
  fresh evidence; shared directories/coverage/type-content-link-metadata conflicts,
  explicit choices and sealed metadata/root/hardlink policy. Canonical16385rows /
  16464B plans, stale row receipts, resolved live binding and88B job/evidence/plan
  comparison keys; decoded history/keys grant no approval or durable barrier.
  Eleven new tests,1436Rust529/112/795+3ignored, all-target Clippy/fmt PASS normal;
  DIRECTORY-SYNC-PLAN.md and assessment detail acceptance. Physical hardlink/
  metadata planner, explicit UI approval/helper ownership, journal/staging/backup/
  publication/queue/crashundo remain pending; installed dry-run unchanged.
  Full goal, resource/startup freeze, preview pause and global gates remain active.

- C02 NODE: `directory_sync_physical` SOURCE ONLY now derives complete selected
  transactions/stage trees/old-preservation scopes and parent/mount references;
  staged hardlink cohorts across transactions, explicit omitted/external counts,
  metadata recipes/unknown-value constraints, dirty parents and old alias ctime
  impacts. All sources must stage before any destination effect.16384T/nodes/old
  entries, MAXwire3145856B, sealed separate staging payload≤1TiB charges independent
  hardlink multiplication and symlink targets.1450Rust543/112/795+3ignored/14new,
  all-target Clippy/fmt PASS normal; DIRECTORY-SYNC-PHYSICAL.md/assessment scope.
  No actual metadata acquisition, physical execution/backup/journal or UI/install.
  Full topology of untouched equal/retained names needs explicit review; selected
  cohorts never rewrite skips. Resolve/seal actual metadata then live ownership/
  approval/journal/ownrevision ledger/staging/publication/queue/crashundo next.
  Installed dry-run and fullgoal/freezes/preview pause/global open gates unchanged.

- C02 NODE: `directory_sync_metadata` SOURCE ONLY reads actual bounded raw values
  through pinned no-follow paths with full revision/mount/ancestor checks; Full/
  Basic recipes and chmod-equivalent Linux ACL adjustment, actual cohort equality
  (including convergent old hashes), separate directory undo/AfterOwnChildren.
  FLSMETv1 streams one≤128KiB value payload at a time,64attrs/131584B codec,
  max32770records/default64MiB/max8GiB artifact budget; no helper stdout increase.
  Final sink mutation/cancel rejects live Acquired after full fresh revalidation;
  decoded/forged history/StoredValues comparison never approval or durability.
  1465Rust558/112/795+3ignored/15new + all-target Clippy/fmt PASS normal. Four initial
  Clippy style lints fixed without suppression; full16384values/rawxattrs/ACL real/
  cohorts/undo/deep substitution/limits/history scope documented in
  DIRECTORY-SYNC-METADATA.md/assessment. Next owned metadata/journal file identity,
  readback/fsync ACK tied job/evidence/logical/physical/values, then explicit UI
  approval/helper pins/ACK, own-effect ledger, global staging/preservation/
  publication/queue/crashundo. Installed dry-run unchanged; FULLgoal/freezes open.

- C02 NODE: `directory_sync_store` SOURCE ONLY creates exclusive owned0600
  FLSSAVv1 bundles in private disjoint0700 directories, with complete canonical
  evidence/choices/stage budget and actual values. SavedIntent rebuilds review
  without original paths or pins; full16384record artifact>8MiB verified.
  Type-first O_PATH readback, file/name/mount/revision/ancestor checks, per-file
  PID-guarded flock, file and child-first ancestor namespace fsync,212B identity/
  intent/value comparison proof; reopen History, failed renewal Uncertain until
  explicit reopen, borrowed reverified artifact-only barrier.1482Rust575/112/795
  +3ignored/17new/Clippy/fmt PASS normal; initial range lint fixed, owned abrupt
  exit three phases and live inherited-forkFD checks. DIRECTORY-SYNC-STORE.md /
  assessment scope. No effects, journal, approval/helper/UI/release/install yet;
  next indexed value access + owned effect journal/approval/ownmutation ledger/
  global staging/recoverable no-clobber barriers/queue/crashundo. Complete goal,
  resource/startup freeze, preview pause and native/global open gates unchanged.

- C02 NODE: SOURCE ONLY indexed Store values now derive offset/span/original-byte
  digest only from full validated intent/metadata readback. Exact expected proof,
  positional one-record read≤131594B, header/type/policy/checksum and post-binding
  verification; no shared cursor or repeated full artifact replay. History and
  Uncertain values remain data, not ACK/Sourcefreshness/approval.32770slot bound,
  no payload retention/persisted offsets/wire-format or stdout-cap changes.
  Physical stage metadata visits each cohort leader once, directories postorder;
  IndependentNames retains every inode.1493Rust586/112/795+3ignored/11new,
  Clippy/fmt PASS normal; DIRECTORY-SYNC-INDEX.md/assessment, full16384records>8MiB,
  reverse/concurrent/offline/empty/corrupt/bounded-span/flags tests. Installed
  dry-run unchanged, no release/helper/UI/new resources. Effect journal/new
  approval/helperpinsACK/ownledger/staging/backups/publication/queue/crashundo
  next; complete goal/freezes/preview pause/native/global open gates unchanged.

- C02 NODE source foundation: sequenced effect declarations and Linux owned
  journal persistence; preparation-bound header, strict digest-chain replay,
  all-stage-before-effects, preserve-before-publish, directory metadata and undo
  ordering. Failed write/ACK latches Uncertain; reopen History and explicit
  renewal. Tickets/observations are declarations, not approval/verification.
  See DIRECTORY-SYNC-JOURNAL.md. Actual executor, own-mutation ledger, native
  approval/helper pins/ACK, global staging/backups/publication and recovery UI
  remain pending. Installed dry-run, complete goal and freezes unchanged.

- C02 NODE source-only own-root-rename reconciliation now rereads opt-in sync
  inventories under new raw prefixes, matches every content/metadata/mount and
  child revision, permits only root ctime change, commits recorded paths only
  after complete success. Trees/hardlinks/symlinks supported without weakening
  legacy EXDEV.1507Rust600/112/795+3ignored/5new, Clippy/fmt PASS; full8193-entry
  case and unrelated hardlink snapshot still strict. DIRECTORY-SYNC-REBASE.md.
  Full alias/parent namespace ledger and positive own-effect provenance remain
  pending, as do actual sync execution, approval/helper pins and recovery UI.

- C02 NODE source-only exact rename transition now reconciles known alias scopes
  as one recorded-state batch: exact old/new revision and values, full rewalks,
  duplicate physical-name detection, shared read allowance, final root checks
  then all-owner commit. Same-inode distinct names remain legal; overlapping
  names and arbitrary ctime exceptions reject. Cooperative worker Control kept.
  1515Rust608/112/795+3ignored/8new, Clippy/fmt PASS; full32772 entries/one-over,
  raw/symlink aliases, stale transition and later-scope failure/cancel coverage.
  DIRECTORY-SYNC-ALIASES.md. Parent namespace/nlink ledger, positive own-effect
  provenance, approval/helper pins, actual effects/recovery and journal scaling
  remain pending; installed dry-run and full goal/freezes/gates unchanged.

- C02 NODE source-only namespace rename/transfer reconciliation now checks one
  or two complete scopes, planned moved subtree and exact parent child sets,
  unchanged attributes and predicted nlink, then commits both recorded owners.
  Candidate shape budgets precede path clones; compact metadata templates keep
  canonical digest unchanged. Separate execution scope16,385/2MiB retained names
  supports full input union/reopen while input8193/1MiB remains unchanged.
  1524Rust617/112/795+3ignored/9new/Clippy/fmt PASS, raw/subtree/alias/attribute/
  atomic-failure/control and full-union/one-over/preflight-bound coverage.
  DIRECTORY-SYNC-NAMESPACE.md. Actual no-clobber own syscalls/provenance, durable
  journal barriers, staged metadata reproduction, visible approval/helper pins
  and recovery UI still pending. Full goal/freezes/native/global gates unchanged.

- C02 NODE source-only owned namespace rename now joins a durable intent with
  exactly one FD-relative RENAME_NOREPLACE, strict post-reconciliation, moved
  data/parent fsync and journal confirmation before a private receipt. Failures
  poison the owner and preserve objects; no implicit retry or rollback. Prepared
  snapshots retain original symlink metadata. 1533Rust626/112/795+3ignored/9new,
  Clippy/fmt PASS; collision/substitution/ten faults/three actual exit83 coverage.
  DIRECTORY-SYNC-OWN-RENAME.md. Trusted coordinator still must establish actual
  global staging, native approval/helper and absolute bindings. Full engine,
  metadata reproduction, recovery/undo UI and quadratic bulk scaling remain open;
  installed read-only sync, full goal/freezes/native/global gates unchanged.

- C02 NODE source-only actual staging barrier verifies ALL transaction workspaces
  against disk and sealed desired node observations, fsyncs pinned regular/dir
  data and containing parents, then fully reverifies under one shared read budget
  before durable WorkspaceSynced/StagingComplete ACKs. Private historical receipt;
  partial ACK poisons owner, reopen never resumes implicitly. Control pause/resume/
  cancel retained.1540Rust633/112/795+3ignored/7new, Clippy/fmt PASS; two actual
  workspaces/partial first ACK/late mutation/fresh unwanted values/read bounds.
  DIRECTORY-SYNC-STAGING-BARRIER.md. Object creation/metadata reproduction, native
  approval/helper/source/absolute-root freshness, full recovery/undo and UI Apply
  remain pending; quadratic scaling, native/global gates and freezes unchanged.

- C02 NODE source-only stage metadata reproduction now applies and exactly
  rereads raw values on pinned owned objects: group, temporary private mode,
  extra-attribute removal, raw/ACL values, final mode and exact times. Held FDs
  protect replacement names; errors retain partial metadata and original
  inventories, with explicit mutation-attempt flags. Control retained.1548Rust
  641/112/795+3ignored/8new/Clippy/fmt PASS; Basic retained attrs+ACL and Full
  hardlinked-tree fixtures precede actual staging barrier/own rename. Identical
  symlink attrs preserved; changes refused without safe FD mutation (NODE O_PATH
  xattrat EBADF); nonempty link profiles have no positive acceptance.
  DIRECTORY-SYNC-STAGE-METADATA.md. Real staged tree creation/durable create-meta
  intents, source/native approval/helper/absolute bindings, recovery/undo/UIApply
  and bulk scaling remain pending; installed dry-run/freezes/global gates intact.

- C02 NODE source-only actual staged-node creation now writes durable tag28
  intent before one exclusive create/mkdir/symlink/held-FD hardlink, ACKs identity
  before file bytes, verifies bounded copy/readback/literals, syncs data/parent
  and ACKs tag29 materialization. Pending guards prevent implicit replay/retry;
  errors retain partials/explicit syscall flags. O_EXCL and held-FD evidence are
  distinguished from directory/symlink named postimages.1556Rust649/112/795
  +3ignored/8new/Clippy/fmt PASS; cross-workspace regular+symlink cohorts, raw
  tree/multichunk/EOF/faults/Control/three real exit89, full sync fixtures now use
  creator→metadata→global barrier→own rename. DIRECTORY-SYNC-STAGE-CREATION.md.
  Workspace root creation/placement and shape/FD admission, durable metadata
  intents, source/native approval/helper/absolute bindings/recovery/undo/UIApply
  and bulk scaling remain pending; installed dry-run/freezes/global gates intact.

- C02 NODE source-only durable staged metadata now uses tag30 before178B and
  tag31 after146B around real FD writes, with per-syscall live journal guards,
  exact Physical metadata order/prior confirmations, complete all-scope pre/post
  verification, inode/parent sync and final guards before atomic alias-owner
  commits. Budget for both passes checked before intent; failure preserves all
  original snapshots and poisons owner after intent writing.1564Rust657/112/795
  +3ignored/8new/Clippy/fmt PASS; two-workspace file/symlink aliases, sixteen faults,
  post-ACK rejection, Control/tamper and three real exit97. Full owned pipeline
  now creator→durable metadata→global barrier→own rename.
  DIRECTORY-SYNC-METADATA-JOURNAL.md. Workspace-root factory/placement/shape-FD
  admission, native approval/helper/absolute/source lifetimes, SharedDir
  AfterOwnChildren, explicit recovery/undo/UIApply and quadratic bulk scaling
  remain pending; installed dry-run/fullgoal/freezes/native-global gates intact.

Checkpoint 2026-11-02: [whole-batch staging admission](DIRECTORY-SYNC-ADMISSION.md)
now checks every `new`+`old` workspace and the destination-name envelope before
new journal creation and owned effects. Retained FD slots are a policy forecast;
workspace placement/factory and actual host/transient FD admission remain open.

Checkpoint 2026-11-02: [owned workspace factory](DIRECTORY-SYNC-WORKSPACES.md)
now creates complete batches under pre-existing private disjoint same-mount
containers, with durable intent/identity/materialization and a point-in-time FD
headroom check. Container selection/creation, integrated coordinator/FD lifetimes
and native/source/helper/absolute approval bindings remain open.

Checkpoint 2026-11-02: [durable placement registry](DIRECTORY-SYNC-PLACEMENTS.md)
now preserves complete raw parent paths/chains and canonical workspace names
before mkdir, anchors the registry seal with tag34 and requires a matching live
renewed registry for registered journals. History inspection remains independent
of saved path availability; artifact attachment supplies no native approval.

Checkpoint 2026-11-02: [shared-container batch preparation](DIRECTORY-SYNC-BATCH-PREPARATION.md)
now groups exact raw parent paths, bounds requested placement metadata before
pinning, shares parent handles and uses the grouped FD forecast. Registry rows
and per-effect absolute/name checks remain intact; no process-resource or native
execution acceptance is inferred.

Checkpoint 2026-11-02: [private staging coordinator](DIRECTORY-SYNC-STAGING-COORDINATOR.md)
now connects complete owned creation, metadata and global barrier with cumulative
payload admission, bounded leader receipt lifetimes, shared parents and fresh
source checks. Destination execution, native approval and recovery are separate
unfinished phases; no installed Apply or bulk/resource acceptance follows.

- C02 NODE source-only batch namespace observer prerequisite now includes all
  private workspaces and original roots in one atomic reconciliation, preserving
  cross-workspace hardlink ctime coherence. Explicit aggregate quotas leave
  endpoint-only and original capture profiles unchanged. See
  DIRECTORY-SYNC-NAMESPACE-BATCH.md. Destination batch orchestration, effect
  budget/absolute/native bindings, shared-directory metadata, recovery/undo and
  UI Apply remain pending; FULL goal and all existing freezes remain active.

- User-authorized100k follow-up: sourceff7af56 wakes both the bounded listing
  producer and FLTK event loop, keeping capacity8/batch256/pump64/generation/
  limiter4.1612Rust/Clippy/fmt pass. NODE100k median206ms vsFlea0.4 408ms,
 32.6vs104.6MiB tree RAM,4runs/group; original603ms retained. M2 separate
  ARM trial:100k213vs195ms(near polling resolution, no Files win), Flea uses
  llvmpipe. M2 first-draw10/10<100ms(method-limited), corrected1080p10kGroups
  memoryFAIL58.46MB peak vs50MB; native input/default file chooser/global
  acceptance remain open. LISTING-EVENT-WAKE.md+assessment document all limits.

- 2026-11-04 Intel comparison, same optimizedff7af56 native binaries,16validruns:
  Corei5-5257U/Iris6100 hardwareVulkan,1256x750client@2x.10kFiles159vsFlea279ms,
 39.0vs56.4MiB treeRAM;100kFiles1420vsFlea271ms,51.8vs58.8MiB. Flea DRMresident
 79.8MiB observed separately, no blind addition toPSS. INTEL-COMPARISON.md.
  Userclarifies50MBobjective is NOT project veto; memorywin remainsvaluable.
  Different host/display configs preclude cross-host ranking;100k Intel weakness
  needs profiling, not a fabricated universal victory or unmeasured cause.

- 2026-11-04 input checkpoint: overlay repaint fix and reusable bounded wheel
  motion live in lib-fltk-omarchy fb581c7 (unpublished), exported exactly under
  vendor with provenance. Files uses the shared explicit flush and scroll
  controller across main/dual/column views. Papelera is pinned in the sidebar
  and opens a bounded read-only home-trash catalog. 1620 workspace tests and
  Clippy pass; mapped X11 functional scene passes release/menu pixels, exact
  wheel travel/return, catalog reload and data preservation. Final native/app
  installation validation is recorded separately. Physical touchpad inertia,
  persistent restore/empty UI, other volumes, Intel100k profiling and the full
  original goal remain open. TOOLKIT-INPUT.md describes the scope and limits.
- Follow-up: this input checkpoint is installed on NODE, 5 backups/hashes
  verified; source X11/Wayland and installed Wayland scenes pass. Receipt
  installed-checkpoint-1793772472114089109.json and TOOLKIT-INPUT.md. No M2/Intel
  replacement, no toolkit publication, no physical touchpad acceptance yet.
