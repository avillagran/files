# Dependencies of 0.0.2

The files code is MIT. Rust dependencies are resolved in `Cargo.lock`.
The review of `cargo metadata --format-version 1` found:

- MIT, Apache-2.0, BSD or an MIT/Apache alternative: the usual dependencies,
  including FLTK Rust, serde, TOML and opener.
- FLTK C++: LGPL with an explicit static-linking exception, accepted in the design.
- Specific exceptions **authorized by the user on 2026-10-26**:
  `notify` → CC0-1.0; `inotify` and `inotify-sys` → ISC;
  `unicode-ident` tables → Unicode-3.0 alongside MIT/Apache-2.0.

The dual expressions `Unlicense OR MIT` and `Zlib OR Apache-2.0 OR MIT` are used
under their MIT alternative. No GPL or proprietary dependencies were added.
The exceptions above do not authorize extending the list to other dependencies.

On Linux, FLTK uses the platform libraries already installed (Wayland,
Cairo/Pango, fontconfig/FreeType, X11 as a fallback). No GTK/QML runtime,
web server, decoders, network stack or models are distributed.

Primary references: [fltk-rs](https://github.com/fltk-rs/fltk-rs),
[FLTK license](https://www.fltk.org/COPYING.php),
[notify](https://github.com/notify-rs/notify),
[inotify-rs](https://github.com/hannobraun/inotify-rs),
[unicode-ident](https://github.com/dtolnay/unicode-ident).


0.0.2 reuses serde_json/libc, already present, for the helper protocol/supervision;
it adds no license families. The four ported shaders keep MIT and their sources/
hashes in assets/shaders/README.md. Their EGL tests use system libraries;
the app still has no GPU context by default and the real integration will be audited when it arrives.

## 0.0.4: image helper (preparation)

`files-helper` adds `image = 0.25.10` with `default-features = false` and only
`png`, `jpeg`, `webp`. The **selected** normal/build tree is saved in
`docs/evidence/image-preview/dependency-audit.json`; it does not enable Rayon, AVIF, EXR,
TIFF, GPU or network. The original manifests of the resolved version were reviewed.

Chosen alternatives: image/image-webp/png/flate2/fdeflate/crc32fast/cfg-if/
num-traits/quick-error/autocfg/bitflags under MIT or Apache-2.0; zune-core and
zune-jpeg under MIT/Apache, not their Zlib alternative; bytemuck and miniz_oxide under
MIT/Apache; byteorder-lite under MIT (not Unlicense); adler2 under MIT/Apache;
simd-adler32 under MIT; moxcms and pxfm under BSD-3-Clause/Apache-2.0. No license
exception is added.

**zlib-rs 0.6.8**, Zlib only, appears as an **optional, unselected** dependency
in Cargo.lock through flate2/png. It is not enabled, built or linked; it is not
considered authorized. The evidence uses `cargo tree -p files-helper -e normal,build`
and the real build logs, not a naive walk of `cargo metadata.resolve`
(which can also include weak optional edges). A change that selects
that backend needs a review and prior authorization of that license.

Format/bounds/limitation details and primary sources in
[IMAGE-PREVIEW.md](IMAGE-PREVIEW.md). These dependencies live only in the
helper; its PSS and that of the integrated app tree still have to be measured separately
before accepting P01/P02. A functional helper does not complete the image UI.

JPEG release upgrade: image features now only `png,webp`; helper-only path
`vendor/jpeg-decoder` 0.3.2, MIT OR Apache-2.0 (Apache option used), default features
OFF, no Rayon. Exact archive/hash/licenses/original src and applied source patch are
in SOURCE.json/PATCHES.md; independent provenance review vendor-source-review.json.
The local manifest declares historical asmjs for modern check-cfg without changing
source. No IJG/TurboJPEG is linked or authorized. image's zune JPEG adapter is no
longer selected by this release; previous audit belongs to the earlier candidate.

## Native Markdown (0.0.4)

`pulldown-cmark =0.13.5`, defaults disabled, MIT license. New transitive dependency
`unicase 2.10.0` MIT/Apache 2; bitflags and memchr already allowed. Native events are used,
not the HTML serializer feature nor CLI/getopts. Manifests/licenses read from the
local registry; checksums pinned in Cargo.lock. No new browser engine nor GPU.

## Optional HTML adapter (0.0.4 experimental)

It adds no crates. litehtml 6fd7aaa and the bundled Gumbo are built statically into a
separate ephemeral executable, BSD-3/Apache-2. Pinned archive 82cfdc83… and licenses,
sources/recipe/hashes in HTML-ENGINE-PROBE and scripts/fixtures/html-engine.
The original UC/Sun strtod routine is NOT built: a patch without fuzz and an own MIT
bridge. The full Hoehrmann MIT notice is preserved after checking the table and decoder
against the primary source. No new exception. `make html-helper` only prepares
local artifacts in target/release; portable packaging and a budget are missing.

## System video tools

P07 uses the optional, already installed ffprobe/ffmpeg through direct processes,
according to DESIGN J. Those executables and their codecs are not redistributed/linked;
there is no new Cargo dependency/download/installation. Licenses of the libraries
compiled into files still follow the allowlist. Paths/version/SHA of the tools
used are recorded in the video-ui JSON; their absence is presented as an unavailable
capability. Their resource use is part of the future total gate and is not hidden
for being an external process. Software video playback and later audio are documented in VIDEO-PREVIEW/AUDIO-PLAYBACK; no new Cargo packages.

## System PDF tools

P06 uses the optional pdfinfo/pdftoppm through direct executables. Poppler/PDF libraries
are neither linked nor redistributed, and Cargo dependencies do not grow. Their
license/updates belong to the OS package; no new exception for
code distributed with files. A portable adaptation/capability without the tools
and the tree's PSS are missing; paths/versions/SHA from the functional scene in the pdf-ui JSON.

## Window gesture adapter (0.0.4)

cc 1.10.0 MIT/Apache and fltk-sys 1.5.23 (already in the lock) become direct
build dependencies to compile the local MIT C++ bridge against the real headers.
No new runtime libraries nor GL/GPU; the existing FLTK license and static exception.
Pin/API/ABI details and sources/config in WINDOW-MODES.md.

## Regex renamer (0.0.4)

`regex = 1.13.1`, defaults OFF and only `std`, with regex-automata 0.4.18 and
regex-syntax 0.8.11 under the MIT alternative (also Apache-2.0). The selected feature
tree is in `evidence/rename-regex/`; `aho-corasick` 1.1.5 appears in the lock
under MIT/Unlicense but is not enabled in this normal tree.

The review of the original files found `regex-syntax/src/unicode_tables/
LICENSE-UNICODE` with Unicode 1991–2018 terms. It is not the specific
unicode-ident exception and it was not implicitly authorized: those tables' features stay
DISABLED. The engine still accepts UTF-8 scalars/literals/ranges; Unicode classes,
properties and casefold show a capability error. Workspace unification
adds only automata's `dfa-search` through the existing bstr/opener; it does not
enable syntax tables nor DFA-build/hybrid/perf. Hashes/manifests/decision in
`dependency-audit.json`. No new exception, download or global installation.

## Opening files — 2026-10-26
opener and its bstr/normpath dependencies were removed when adopting the system `open`
by explicit instruction. No new dependency nor fallback. The transitive
dfa-search feature of regex-automata that bstr used to bring also disappears;
regex stays std/NFA/syntax without unauthorized Unicode tables. The earlier section
is the history of that checkpoint; the current tree is in evidence/open-close/regex-features.txt.

## PNG by rows — 2026-10-27
`png = 0.18.1`, MIT OR Apache-2.0, already transitive through image; only
the direct dependency in the helper is added. No new package nor native/GPU decoder.
The row/Adam7 reader and limits are described in PNG-ROWS-PREVIEW; local caps
are not equivalent to the tree's PSS budget.

Selected audio: production uses the same OS ffmpeg with Pulse output,
and does not link or redistribute ffplay/SDL. The ffplay prototype remains only in git history/
evidence because of its failed native EOF. Current ffprobe/ffmpeg paths/version/SHA
in audio-ui 1793086873981565834; absence/unavailable output are visible errors.
The process's resource use belongs to the pending total gate; AS 768 MiB virtual is NOT PSS.


GIF M16 adds no crate nor linked engine: it only uses the gif demuxer/codec of the system
ffmpeg/ffprobe, already optional, with a per-format whitelist. It does not redistribute
the CLIs. The GIF-PREVIEW caps and functional validation do not prove the tree's PSS.

## DOCX in preparation (0.0.4)

quick-xml =0.41.0 MIT, defaults off, only the already existing memchr dependency.
flate2 =1.1.10/crc32fast =1.5.3 become direct helper dependencies; flate2
defaults off/rust_backend keeps the permissive miniz_oxide, NOT zlib-rs. The
normal/build tree offline/locked and the manifest/license hashes in
evidence/0.0.4/office-dependency-audit.json. No new license family,
DOM/browser/network/daemon. This is a static audit, NOT UI/PSS acceptance.

## EXDEV source inventory (0.0.4, M25 WIP)

Incremental SHA256: `sha2 =0.10.9`, defaults off, feature std. Eight new packages
in the lock: sha2, block-buffer, cpufeatures, crypto-common, digest, typenum
MIT/Apache 2; generic-array MIT; version_check MIT/Apache 2. Declarations and
local LICENSE files checked in exdev-dependency-declarations.json;
the coordinator independently verified the same declarations. No new
exception. This does not imply a full source audit nor an app budget.
sha2-asm is not selected; the normal/build tree is checked before the checkpoint.


## M29a — Wayland MIME (0.0.4 partial)

No Cargo.lock changes nor new dependencies. The local MIT C++ bridge compiles
against the private headers of the already pinned FLTK 1.5.23 and the already linked Wayland; a static_assert
on ABI/API requires a review when updating. It does not redistribute an external manager nor use
data-control. FLTK keeps its already documented license/linking exception. A possible
future DnD hook needs a reproducible source/patch for that pin, not editing the registry.


## M29b — FLTK 1.5.23 Wayland hook

The upstream TU and a bounded patch are included in vendor/fltk-wayland-transfer under the
FLTK LGPL license with the static exception and the full COPYING; FLTK modifications
are distributed under those terms, not MIT. SOURCE.json pins the SHA of the crate archive,
source, patch and license. No new crates, licenses or network; the CMake wrapper only
modifies source generated in target. Guard and rebuild documentation in the vendor's
README. Building does not prove native interop nor memory use.

## 2026-11-04 — vendor lib-fltk-omarchy 1ff1adf

The toolkit update (Omarchy tooltips, animations, RTL/i18n)
adds `unicode-bidi 0.3.18` and `unicode-segmentation 1.12.0` to the lockfile,
both `MIT OR Apache-2.0`, with no additional Unicode license files or terms
(LICENSE-* and sources reviewed). `pulldown-cmark =0.13.5` still
resolves with the toolkit's ≥0.13.5 requirement.
