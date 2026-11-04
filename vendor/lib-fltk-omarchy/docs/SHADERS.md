# Shapes, paints and shaders on CPU or GPU (0.0.6)

`fltk_omarchy::gfx` draws custom visuals in **software**, on the CPU: no
GPU, no OpenGL and no extra fltk features. An optional GPU path exists
behind the `gpu` crate feature, **off by default** (see [GPU](#gpu)). The crate keeps
`use-wayland` and `no-images`, so an app that depends on it builds exactly
as before. Colors come from the theme through `Uniforms`, so every visual
follows `omarchy-theme-set`.

![gallery](components/gfx-gallery.png)

```sh
cargo run --example shaders                                  # the window above, animated
cargo run --example shaders -- --snapshot docs/components    # re-render these pictures
```

## The five pieces

| Piece | What it is |
|---|---|
| `Path` | Vector shape: `move_to`, `line_to`, `quad_to`, `cubic_to`, `arc`, `close`; `rect`, `rounded_rect(x, y, w, h, [tl, tr, br, bl])`, `circle`, `ellipse`, `polygon`, `squircle(x, y, w, h, n)`; `translate`, `scale`, `rotate`; `contains(x, y)` and `near_outline(x, y, width)` for hit-testing |
| `Canvas` | Pixel buffer you draw on: `fill(&path, paint)`, `stroke(&path, &Stroke, paint)`, `clip(&path)` / `clipped(&path, \|c\| ..)`, `layer(opacity, blend, \|c\| ..)`, `shadow(&path, dx, dy, blur, color)`, `blur(x, y, w, h, radius)`; fields `opacity` and `blend` |
| `Paint` | `Color` (solid), `Paint::linear(from, to, stops)`, `Paint::radial(center, radius, stops)`, `Paint::conic(center, angle, stops)` |
| `Shader` | Any `Fn(x, y, &Uniforms) -> Color`, run for every pixel. `Uniforms` gives `time`, `frame`, `size`, `pointer`, the theme `background`, `foreground`, `accent` and `urgent`, and four free `params` |
| `View` | The widget: `.paint(\|c, u\| ..)`, `.shader(s)`, `.post(\|c, u\| ..)`, `.animate(fps)`, `.max_area(px)`, `.budget_ms(ms)`, `.hit_shape(Hit::Fill(path))`, `.on_click(f)`, `stats()` |

`Stroke::new(width).join(Join::Round | Join::Bevel | Join::Miter(limit)).cap(Cap::Butt | Cap::Round | Cap::Square)`.
`BlendMode`: `Normal`, `Multiply`, `Screen`, `Add`, `Darken`, `Lighten`.
`Color::hex("#e68e0d")`, `.with_alpha(a)`, `.mix(other, t)`; `Color::from(theme_rgb)`.

## Shapes

![shapes](components/gfx-shapes.png)

```rust
use fltk_omarchy::gfx::{Cap, Join, Path, Stroke, View};
let shapes = View::new(560, 150).paint(|c, u| {
    c.fill(&Path::rounded_rect(10.0, 20.0, 110.0, 110.0, [28.0, 4.0, 28.0, 4.0]), u.accent);   // radius per corner
    c.fill(&Path::squircle(140.0, 20.0, 110.0, 110.0, 4.5), u.accent.mix(u.background, 0.35));
    c.fill(&star(325.0, 75.0, 58.0, 26.0, 5), u.accent.mix(u.foreground, 0.3));          // Path::polygon (star(): examples/shaders.rs)
    let wave = Path::new().move_to(400.0, 110.0).cubic_to(430.0, 10.0, 470.0, 140.0, 545.0, 40.0);
    c.stroke(&wave, &Stroke::new(10.0).cap(Cap::Round), u.foreground.with_alpha(0.35));
    c.stroke(&wave, &Stroke::new(3.0).cap(Cap::Round), u.accent);
    let zig = Path::new().move_to(400.0, 135.0).line_to(430.0, 115.0).line_to(460.0, 135.0).line_to(490.0, 115.0);
    c.stroke(&zig, &Stroke::new(6.0).join(Join::Round).cap(Cap::Round), u.foreground);
    c.stroke(&zig.translate(60.0, 0.0), &Stroke::new(6.0).join(Join::Miter(4.0)), u.foreground.with_alpha(0.6));
});
```

Every edge is anti-aliased. Fills use the non-zero winding rule.

## Paints

![paints](components/gfx-paints.png)

```rust
use fltk_omarchy::gfx::{Paint, Path, View};
let paints = View::new(560, 130).paint(|c, u| {
    let (a, b, f) = (u.accent, u.background, u.foreground);
    c.fill(&Path::rounded_rect(10.0, 10.0, 170.0, 110.0, [12.0; 4]),
           Paint::linear((10.0, 0.0), (180.0, 0.0), &[(0.0, a), (1.0, b.mix(f, 0.15))]));
    c.fill(&Path::circle(285.0, 65.0, 55.0), Paint::radial((270.0, 50.0), 70.0, &[(0.0, a.mix(f, 0.5)), (0.6, a), (1.0, b)]));
    c.fill(&Path::circle(450.0, 65.0, 55.0), Paint::conic((450.0, 65.0), -PI / 2.0, &[(0.0, a), (0.5, f.mix(b, 0.5)), (1.0, a)]));
    c.fill(&Path::circle(450.0, 65.0, 30.0), b);
});
```

## Effects: shadow, layer opacity, blending, clip

![effects](components/gfx-effects.png)

```rust
use fltk_omarchy::gfx::{BlendMode, Color, Path, View};
let effects = View::new(560, 150).paint(|c, u| {
    let card = Path::rounded_rect(20.0, 20.0, 140.0, 100.0, [14.0; 4]);
    c.shadow(&card, 0.0, 8.0, 12, Color::rgba(0.0, 0.0, 0.0, 0.6));       // blur ≤ 32 px
    c.fill(&card, u.background.mix(u.foreground, 0.12));
    c.layer(0.6, BlendMode::Normal, |c| {                                   // group opacity
        c.fill(&Path::circle(225.0, 70.0, 40.0), u.accent);
        c.fill(&Path::circle(265.0, 70.0, 40.0), u.accent);
    });
    c.blend = BlendMode::Add;                                               // additive glow
    c.fill(&Path::circle(370.0, 60.0, 34.0), u.accent.with_alpha(0.7));
    c.fill(&Path::circle(395.0, 85.0, 34.0), u.foreground.with_alpha(0.5));
    c.blend = BlendMode::Normal;
    c.clipped(&Path::squircle(450.0, 20.0, 100.0, 110.0, 4.0), |c| { /* stripes */ });
});
```

## Shaders: write once, the toolkit picks GPU or CPU

Write the effect once with `gfx::sl`, the portable shader language: Rust
functions that build the shader. The toolkit runs it on the **GPU**
(generated GLSL) when the app was built with the `gpu` feature and a GL
context works. Otherwise it runs on the **CPU** (a row interpreter). The
app code and the result are the same on both:

```rust
use fltk_omarchy::gfx::{self, sl::*, Rate, View};
// Built-ins (portable):
let noise = View::new(130, 110).shader(gfx::shaders::noise()).animate(Rate::Display);        // fbm noise, theme bg → fg
let drift = View::new(130, 110).shader(gfx::shaders::gradient()).animate(Rate::Display);     // bg → accent, drifting
let glow  = View::new(130, 110).shader(gfx::shaders::accent_glow()).animate(Rate::Display);  // breathes; follows the pointer
let rings = View::new(130, 110).shader(gfx::shaders::rings()).animate(Rate::Display);
// Your own:
let pulse = View::new(200, 200).shader(Shader::new(|p, u| {
    let d = length(p - u.size() * 0.5) / min(u.size().x(), u.size().y());
    mix(u.accent(), u.background(), smoothstep(0.0, 0.5 + 0.05 * sin(u.time() * 2.0), d))
})).animate(Rate::Display);
```

`sl` in short:
- **Values:** `F` (float), `V2` (position/size) and `V4` (RGBA), with
  `+ - * /`. A float works with any of them.
- **Functions:** `sin cos abs floor fract sqrt exp pow min max clamp mix
  step smoothstep length distance dot atan2 vec2 rgba`, `.x() .y()`,
  `.r() .g() .b() .a()`, `.with_alpha(a)`.
- **Noise:** `hash`, `noise` and `fbm(p, octaves)`. They use an integer
  hash, so the GPU and the CPU draw the same pattern.
- **Uniforms (`u`):** `time()`, `size()`, `pointer()` (−1 when outside),
  the theme colors `background()`, `foreground()`, `accent()`,
  `urgent()`, and `param(0..4)` (`View::set_params`).

**How the toolkit decides:**
- **GPU** when the View has a portable shader, no `post`, the app was
  built with `--features gpu`, and the GL layer started.
- **CPU** otherwise. `view.active_backend()` says which one is in use and
  `view.backend_reason()` says why it is not on the GPU.
- To force one, for a special reason: `.backend(Backend::Cpu | Gpu)`, or
  for the whole app `gfx::set_backend(..)` or `OMARCHY_FLTK_RENDER=cpu|gpu|auto`.

**Frame rate:**
- `animate(Rate::Display)` follows the refresh rate of the monitor showing
  the window: 120 fps on this machine's 120 Hz panel, read from Hyprland
  and re-checked every 3 s; 60 Hz without Hyprland.
- On the CPU it is capped at 30 fps by default (`gfx::set_cpu_fps_cap(n)`),
  because every CPU frame costs CPU time.
- `animate(30)` asks for a fixed rate.

Measured on the same window, same shader (`noise`, 484×300, 3 runs each,
`gpu_bench display`; data in `docs/gpu-display-2026-10-28.jsonl`):

| Backend picked | fps | CPU | CPU per frame | PSS |
|---|---|---|---|---|
| Auto → GPU (`--features gpu`) | 120.0 (monitor: 120 Hz) | 2.1–2.5 % | 0.17–0.21 ms | 60.1 MB |
| CPU (forced, or built without `gpu`) | 30.0 (CPU cap) | 60.8–63.2 % | 20.4–21.1 ms | 18.7 MB |

Same picture on both: the GPU read-back matched the CPU evaluation
exactly at three points. A `paint()` rectangle drawn on top showed through
the GPU overlay (`docs/gpu-parity-2026-10-28.json`).

On the CPU the `sl` interpreter is slower than a hand-written Rust closure
(about 21 vs 14 ms per frame for this noise, in different sessions). For
CPU-only effects `sl` cannot express, use `.cpu_shader(|x, y, u| ..)`,
which always runs on the CPU. The previous closure built-ins are in
`gfx::shaders::cpu`.

### Background blur

```rust
let frosted = View::new(130, 110).shader(gfx::shaders::blobs())
    .post(|c, u| {
        c.blur(15, 25, 100, 60, 10);
        c.fill(&gfx::Path::rounded_rect(15.0, 25.0, 100.0, 60.0, [10.0; 4]), u.background.with_alpha(0.25));
    })
    .animate(Rate::Display);      // post() needs the pixels on the CPU: this View renders on the CPU
```

"Background blur" means blurring what the same View drew: its shader, or
what `paint` drew before. It cannot blur other widgets, because FLTK gives no
cheap access to the pixels behind a widget.

### Several cores

`View::threads(n)` shades bands of rows on `n` threads (0 = the machine's
cores, at most 8; default 1). The frame finishes sooner. Total CPU stays
about the same: in the measurement below it went down a little. The
shader must be `Send + Sync`: closures that capture nothing, or only
`Copy`/`Arc` data, qualify. A pixel test checks that 2, 3 and 8 threads
give the same pixels as one.

```rust
let noise = View::new(400, 300).shader(gfx::shaders::noise()).threads(4).animate(30);
```

### Limits that keep the UI responsive

- **Area limit** (`max_area`, default 160 000 px): a larger View renders
  at a lower resolution and is scaled up when drawn. Shaders and `paint`
  still get widget coordinates. Memory per View is bounded by it: two
  4-byte buffers plus one upload buffer.
- **Time budget per draw** (`budget_ms`, default 6 ms): shaders render in
  32 × 32 tiles. When a frame needs more time, it continues on the next
  draw and the last complete frame stays on screen. The first frame always
  completes.
- **Buffer reuse**: the frame buffers and the upload buffer are kept
  between frames and given back when the View shrinks a lot. Opaque
  shader output takes an RGB fast path.
- **Animation** (`animate(fps)`, ≤ 120) uses an fltk timer, never a loop.
  While the View or its window is hidden, ticks are skipped: no render,
  0.5 s polling.
- **Regions**: a static View renders only when it is invalidated (theme
  change, `set_params`, `invalidate()`, the pointer moving over a shader
  View). An animated View re-renders all of its pixels each tick.
- `stats()` reports `frames`, `partial_draws`, `last_frame_ms`,
  `last_blit_ms`, `render_size`, `paused_ticks` and `memory`.

## Non-rectangular widgets

```rust
use fltk_omarchy::gfx::{Hit, Paint, View};
let shape = star(60.0, 45.0, 42.0, 20.0, 6);
let button = View::new(120, 90)
    .paint({ let s = shape.clone(); move |c, u| c.fill(&s, u.accent) })
    .hit_shape(Hit::Fill(shape))          // or Hit::Outline(path, width)
    .on_click(|| println!("inside the star"));
```

Outside the shape, mouse events go to whatever is below, as if the View
were not there. With `on_click` the View is also a keyboard control: Tab
reaches it, Enter or Space press it, and the focus ring follows the shape.

## GPU

With the crate feature `gpu`, `View` puts portable shaders on the GPU by
itself (see above). `gfx::gpu::GpuView` remains for the special case of
hand-written GLSL. The feature is **off by default**:

```toml
lib-fltk-omarchy = { git = "https://github.com/avillagran/lib-fltk-omarchy", tag = "v0.0.6", features = ["gpu"] }
```

```rust
use fltk_omarchy::gfx::gpu::GpuView;
let glow = GpuView::new(400, 300, r#"
    vec4 shade(vec2 p) {                       // p: widget pixels, y down
        float d = length(p - u_size * 0.5) / min(u_size.x, u_size.y);
        return mix(u_accent, u_background, smoothstep(0.0, 0.5, d));
    }
"#).animate(60);
// Uniforms: u_time, u_size, u_pointer (−1 outside), u_background, u_foreground,
// u_accent, u_urgent, u_params. Compile errors: glow.last_error().
```

Why it is opt-in:
- **Features.** The `gpu` feature turns on fltk's `enable-glwindow`
  (fltk-sys builds FLTK with OpenGL) and adds the `glow` crate (MIT /
  Apache-2.0 / Zlib). Cargo unifies features, so an app that enables it
  changes the fltk-sys build of the whole program. Without the feature,
  fltk-sys keeps exactly `default`, `use-wayland` and `no-images`.
- **Memory.** Loading the GL driver costs about 35 MB of PSS, whether the
  view animates or not (table below).
- **A FLTK limitation on Wayland.** A GL subwindow can only start once its
  window is on an output. FLTK waits for that on the subwindow's first
  draw. On a hidden or inactive workspace that never happens, and the app
  blocks there until the window becomes visible. `GpuView` avoids blocking
  its own first frame: it stays hidden until its window has been shown for
  a few frames, then lays itself out. A window that starts on a workspace
  nobody is looking at still waits until it is viewed. Offscreen snapshots
  cannot capture GL output, so tests use `read_pixel(x, y, f)`.

Checked in a real window (`gpu_bench check`): the left half of the test
shader read back the theme accent `[230,142,13]` and the right half the
background `[18,18,18]`. A broken shader reported Mesa's compile log and
showed the theme background.

## Cost (measured)

### GPU vs CPU (0.0.5)

Same session on 2026-10-28, Asahi Linux (Apple M2, Mesa), Hyprland,
release build. The setup: a 520×400 window with one 484×300 view running
the same 3-octave noise, on the current workspace (floating, without
focus; GL needs a visible window). `script/gpu-bench`, 3 runs of 5 s each;
raw data in `docs/gpu-bench-2026-10-28.jsonl`. The table shows medians;
PSS includes SwapPss and covers the whole process tree.

| Case | fps | CPU | CPU per frame | Frame render | PSS |
|---|---|---|---|---|---|
| CPU `View`, static | — | 0.0 % | — | — | 15.9 MB |
| CPU `View`, animated, 1 thread | 30 | 52.8 % | 17.6 ms | 10.3 ms | 16.5 MB |
| CPU `View`, animated, `threads(4)` | 30 | 45.1 % | 15.1 ms | 3.9 ms | 16.6 MB |
| `GpuView`, static | — | 0.0 % | — | first draw 22 ms (compile) | 51.2 MB |
| `GpuView`, animated | 30 | 0.7 % | 0.24 ms | 0.08 ms to submit | 51.2 MB |

- The GPU path costs almost no CPU, but takes about 35 MB more memory and
  adds the OpenGL build. The release binary was 132 KB larger.
- `threads(4)` cut the frame render from 10.3 to 3.9 ms. Frames that
  needed two draws because of the 6 ms budget went from about 200 to about
  12 per run.
- Rule of thumb: decorative animation in an app that already uses GL →
  `GpuView`. Everything else → CPU `View`, small `max_area`, low fps.

### CPU shaders, earlier session (0.0.4)

Same session on 2026-10-28, Asahi Linux (Apple M2), Hyprland, release
build (`opt-level = "s"`, thin LTO). The setup: a 520×400 window with one
View rendered at 484×300, on a hidden workspace, 3 runs of 5 s each
(`script/gfx-bench`; raw data in `docs/gfx-bench-2026-10-28.jsonl`). The
table shows medians. PSS includes SwapPss and covers the whole process
tree.

| Case | fps | CPU | CPU per frame | View render | Blit | PSS |
|---|---|---|---|---|---|---|
| Static (rendered once) | — | 0.0 % | — | 5.1 ms once | 1.6 ms | 20.5 MB |
| `noise` animated | 30 | 43.2 % | 14.4 ms | 11.1 ms | 3.4 ms | 19.3 MB |
| `noise` animated, `max_area(40_000)` (254×157) | 30 | 28.3 % | 9.4 ms | 5.2 ms | 4.0 ms | 17.9 MB |
| `gradient` animated | 30 | 35.7 % | 11.9 ms | 2.8 ms | 2.1 ms | 19.4 MB |

How to read it:
- Animation costs CPU, not memory: PSS was the same as the static View
  (19.3–20.5 MB in both).
- A full-size animated shader at 30 fps uses 35–45 % of one core. Lower
  `max_area` or the fps for decorative backgrounds.
- "CPU per frame" covers the whole process: shader, blit, fltk and Wayland
  flush, and the extra draws when a frame exceeded the budget. The View
  render and blit columns are the View's own share.

Not measured in that session: GPU and multi-core shading (both added in
0.0.5, measured above) and other machines.
