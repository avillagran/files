# Embedded media: publication and invalidation — 0.0.4 (2026-10-27)

Continues RELATIVE-SOURCE and the existing engines. This unit covers local
Markdown with images; it does not add HTML resources, an LRU across documents or a
measured budget for every format. Resources/startup NOT_MEASURED: explicit
freeze in effect. No new 0.0.4 tag nor global PASS.

## Publication order

1. The text helper returns the root FD identity and the file's effective revision;
   the image helpers apply ProbeRelativeFile→PreviewRelativeImageChecked.
   Expected before reading, FD/name/root revalidated after decode.
2. The supervisor builds a manifest of up to 8 normalized destinations and the
   proofs of the admitted bitmaps. Missing/rejected destinations keep their
   intent, without RGB, so their recreation can be detected. Occurrences share an Arc.
3. The UI keeps the hidden candidate in Loading. It changes token/generation and hands
   the plan's Arc to the SAME reusable metadata worker of the application.
4. That worker attaches directory watches BEFORE the final publication
   probe, and reopens/verifies the document and each admitted resource
   through their expected signatures. The earlier read/decode was not yet Ready.
   Token/epoch must still be the same after probing; dirty is not cleared.
5. Only the response for that destination/epoch turns the candidate into visible Markdown.
   Guard.poll also runs before painting: atomics/slots, never stat on the UI.

An observed modification cancels the generation and removes document/RGB before
loading again. A late response from A does not bring back the discarded document. Navigating,
closing Space or closing the window cancels and waits for the helper's ACK/reap before the
next job. Hiding reported by FLTK suspends demand; real minimization
with OS events is not validated yet.

There is no atomic snapshot of multiple files and no content hash. The epoch
protects changes already observed; an event not yet delivered may need the
next cycle or reconciliation. Without a callback, instant removal is not promised.

## Watches, limits and recovery

Plan at most 8 destinations, 32 directories/64 KiB of watch names; manifest
names <=68 KiB (document, intents and admitted proofs), each path <=4096 B.
The Arc shares metadata with the worker; the plan keeps no bitmap. Limits
checked before joins/registration; no tree is walked to preload it.
Vec/PathBuf capacity and watcher/backend metadata are not PSS nor a measured
budget: these caps do not prove the tree's 50 MB/150 MB.

The root, the root's parent and the ancestors of resources/document are watched. Events
unrelated to the root are filtered so harness logs/captures/config do not reload the document.
Currently an event for another file inside the same root can conservatively
invalidate the document; refining that filter without losing renames is pending.
A missing directory for an optional image keeps the placeholder and reconciliation;
a watch failure on the root/document is visible and pauses decodes, without a retry loop.
Failures of optional watches are covered by reconciliation; individual UI diagnostics
for those failures are missing. That coverage is not presented as complete inotify.

Every 1 s while visible, directories are re-attached and up to 9 metadata results
(root/file FD or error) are observed outside the UI. Failure observations do NOT authorize Ready:
they only detect an absent→recreated/replaced transition and trigger a fresh read.
This allows recovering a renamed/replaced root even if the earlier watch no
longer exists and callbacks are lost. The stale manifest kept during an error
contains only metadata, never the old RGB. A new destination replaces the
pending request; a stuck syscall does not create another probe thread per navigation.

Relative resources keep openat NOFOLLOW on all their components. A document
opened directly keeps earlier symlink parents; the relative context stays
strict. Notify watches and reads are not a sandbox: there is no confinement
of mountpoints/hardlinks nor global protection of the process's other accesses.

No new disk cache, only RAM/pipes. No files next to the original, no fake TTL
collector and no promised deletion with the app off. Own operations trigger
invalidation through the existing refresh/ACK; the scenario of an own operation on an
embedded resource is still pending, separate from the already validated standalone Trash.

## Evidence

442 Rust 141 core/47 helper/254 UI PASS, Clippy workspace all targets -Dwarnings and release
clean. `target/document-watch-test.log`, `document-watch-clippy.log`,
`document-watch-release.log`. Deterministic regressions: replacement between decode and
publication, hidden candidate/dirty/late verification/close, watch error without a
loop, hidden document without spawn, missing/recreated resource, limits, direct alias
vs strict relative context, and a real watcher with callbacks suppressed.

`evidence/document-media/document-media-ui-1793076116463397564.json`: bounded
functional PASS, 2 instances (normal notify and callbacks suppressed). In both: red→blue when
replacing the same name/size/mtime with a new inode; yellow modification with the same
inode/different revision; deleting removes the RGB and shows a placeholder; recreating green;
rename away/back; missing root→new root recovered with a new root FD. The old
pixels disappear in the framebuffers; the effective Proof matches the fixture's stat.
Closing Space started no helpers during the observed interval; reopening requests a new
signature. 5 captures 1000×650. Internal handlers + own filesystem + framebuffer, NO OS input.

Both windows callback→close_requested→ACK→exit 0; 26 helper PID/starttime
identities per instance absent/reused on exit. This is coverage of registered
identities, NOT sampling of every lifetime nor of resources. Readonly worker:
ACK stop false in the first instance, true in the second. A full drain of the
first is not claimed: close requests stop without a join on the UI. An uninterruptible
kernel syscall can delay even the termination; offline/stuck mounts
remain unvalidated. The probe stop is distinguished from the helpers' reap.

Earlier scenarios 1793075662414294903/1793075748600705450 are kept with
sidecars: the original checker accepted a zombie as not alive, without recording whether that
state happened; it does not prove strict absence at that moment. No real zombie was observed
or attributed to the product. The final scene rejects any identical
PID/starttime, even Z, and passes strict absence/reuse. It also adds a
fixed size and in-place changes; it is not a hunt for a resources PASS. Regression
`evidence/0.0.4/markdown-embedded-ui-1793075764251781328.json` PASS for bitmaps,
dedupe/local budget/context/escape/close; no sampler, root-only cleanup.
The harness's pre-existing diagnostic timings are not performance gates.

c10bd74 installed with consistent backup/hash in
`evidence/0.0.4/installed-checkpoint-1793076271320347111.json`:
files 3c6f0c78ee808d759581e0718cdc2d88abbe0c217d4d2ade26aece9db9ddef1e;
helper fcf3a913c39b5173137a904d435a166bb7ae3c39052c17e382905786e3ca9fa7;
HTML a5c37981b28b73c95b443122748b46345b398b07b54941ad1d96d9378726e111.
