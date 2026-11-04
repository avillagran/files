# Optional GLES effects

These are the actual four MIT shaders ported from the local
`omarchy-t3-code/ui/assets/shaders` sources. `sources.json` records the source
commit and per-file SHA-256 hashes (including local source contents); `LICENSE`
retains the original notice. They are **not yet connected to the file manager**.

The GLES 3 port replaces Qt's uniform block with individual uniforms, drops its
unused matrix, and uses `quad.vert` with a three-vertex fullscreen triangle.
Descending `smoothstep` calls are expressed as `1 - smoothstep(low, high, x)`.
The visual algorithms, styles and state inputs remain from the original shaders.

## Host contract

- Bind the target region as the viewport and issue `glDrawArrays(GL_TRIANGLES, 0, 3)`.
  Set all uniforms on each effect activation; GL's zero defaults are not valid
  defaults for the effects. Attribute buffers are unnecessary for this vertex shader.
- `u_size`, radius, origins, widths and scroll use consistent pixel units.
  Size must be positive, opacity and progress/state envelopes lie in `[0,1]`;
  clocks/seed and all other values must be finite.
- Output is premultiplied RGBA. Composite with `ONE, ONE_MINUS_SRC_ALPHA`.
  `source` for reveal/activity must also contain premultiplied pixels and use
  clamp-to-edge. Bind the sampler explicitly to the selected texture unit.
  The host must align texture orientation and local UV coordinates.
- `threadaura`: working/attention/burst envelopes, style selectors, clock, radius,
  seed, intensity and three palette colors. Apply to the operation/status region.
- `reveal`: progress, style (`0` nebula, `1` warp, `2` fold), axis/reverse flags,
  seed, size, glow and source texture. Host should bypass at fully shown and hide
  at fully closed, releasing transient source captures after the transition.
- `starfield`: clock/drift/scroll/origin, meteors/brightness/nebula/fade/ink and
  palette colors. No source texture. Continuous updates must stop while hidden
  or disabled; this asset does not implement that lifecycle itself.
- `activity`: progress, positive `u_textWidth` and `u_bandWidth`, opacity and a
  cached glyph source texture. Original default band is 72 pixels and cycle 2.2s.
  It is a moving brightness band, not a replacement for the static base text.

## Validation and remaining integration

Run `timeout 20s python3 scripts/shader-check.py`. It compiles and links each
shader with the vertex program in an EGL GLES3 pbuffer, rejects known software
renderers, reports the actual renderer and deletes programs/context on exit.
`docs/evidence/shader-compile.json` records the Apple M2 Pro result.

Run `timeout 25s python3 scripts/shader-render-check.py` for 384×128 real GPU
fixtures. It verifies visible nonconstant output, premultiplied alpha, state
changes, zero-opacity transparency, all three reveal styles at full extent,
and aura attention/burst states. It exports straight-alpha PNGs (converted from
premultiplied GPU bytes) to `docs/evidence/shaders/`; its JSON belongs in
`docs/evidence/shader-render.json`. Source textures are synthetic gradients,
not application content. This adds rendering evidence without proving UI integration.

Still required: bounded source textures, actual FLTK/Painter integration,
explicit opt-in and CPU fallbacks, visual tests for states/styles/alpha,
resize/HiDPI/cancellation/hidden lifecycle, and app-plus-helper CPU/memory/GPU
measurements with effects on/off. Compilation is not a rendered-feature claim.
