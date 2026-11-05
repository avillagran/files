# HTML preview candidate

Coordinator research, 2026-10-26. It is not an adopted dependency or an
implemented feature; its resource use in files has not been measured.

## Primary evidence

- [litehtml README](https://github.com/litehtml/litehtml/blob/master/README.md):
  an HTML/CSS layout engine, independent of the graphics backend. Partial
  HTML/CSS support; it does not promise parity with a modern browser. UTF-8 strings.
- [License](https://github.com/litehtml/litehtml/blob/master/LICENSE): three-clause
  BSD. The README identifies Gumbo as Apache-2.0; check the exact transitive
  version and its license before bringing in code.
- [document_container](https://github.com/litehtml/litehtml/blob/master/include/litehtml/document_container.h):
  callbacks for fonts, text, images, CSS, links, viewport and clipping. The app
  must supply drawing and resource resolution.

## Integration proposal to evaluate

- Isolate parser/layout in an ephemeral helper. Small C++ adapter with a bounded
  C interface; share only limited drawing commands or a viewport bitmap.
- Default Nerd Font like the rest of files. Do not enumerate the whole
  catalog nor keep the full document in the main process.
- No JS engine. The host must reject scripts, automatic navigation, remote
  resources and active protocols; do not infer safety from the engine's name alone.
- Resolve CSS/images through local capabilities restricted to the allowed
  directory. Check canonicalization, symlinks, URIs and TOCTOU; block escapes.
- Bound input, nodes, depth, CSS, decoded pixels, time and output
  before large allocations. Cancel and reap the helper when the file changes.
- Markdown can be converted to bounded HTML and reuse the same renderer;
  text extraction alone does not satisfy a rendered preview.

## Required acceptance

Fixtures with headings, lists, tables, links and relative images; hard CSS and
HTML must degrade visibly. Test that network/scripts
and escapes are blocked. Real capture, select/copy, scroll, cancellation and measurements of the
whole process tree. Compare latency and memory with the project's limits before
choosing it. Keep source/version/checksum reproducible and license notices.
