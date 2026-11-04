#version 300 es
precision highp float;
// Port of omarchy-t3-code. Copyright (c) 2026 Andrés Villagrán. MIT; see LICENSE.
// Show/hide transition for T3Reveal. `progress` runs 0 (hidden) .. 1 (shown);
// the visible extent along the axis is [0, progress] measured from the edge
// the region is attached to.
//  style 0 nebula - dissolves into star dust behind a glowing front
//  style 1 warp   - squeezed into hyperspace light streaks, RGB split
//  style 2 fold   - folds like an accordion with lit creases
in vec2 v_uv;
layout(location = 0) out vec4 fragColor;
uniform float u_opacity;
uniform float u_progress;
uniform float u_style;
uniform float u_horizontal;
uniform float u_reverse;
uniform float u_seed;
uniform vec2 u_size;
uniform vec4 u_glow;
uniform sampler2D source;

float hash12(vec2 p) {
    vec3 p3 = fract(vec3(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

float noise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash12(i), hash12(i + vec2(1.0, 0.0)), f.x),
               mix(hash12(i + vec2(0.0, 1.0)), hash12(i + vec2(1.0, 1.0)), f.x), f.y);
}

float fbm(vec2 p) {
    return 0.55 * noise(p) + 0.3 * noise(p * 2.1 + 7.3) + 0.15 * noise(p * 4.3 + 1.7);
}

// Texture coordinate from (along, across), undoing the axis mapping.
vec2 toUv(float along, float across) {
    float a = u_reverse > 0.5 ? 1.0 - along : along;
    return u_horizontal > 0.5 ? vec2(a, across) : vec2(across, a);
}

vec4 tap(float along, float across) {
    if (along < 0.0 || along > 1.0 || across < 0.0 || across > 1.0) return vec4(0.0);
    return texture(source, toUv(along, across));
}

void main() {
    vec2 uv = v_uv;
    float along = u_horizontal > 0.5 ? uv.x : uv.y;
    float across = u_horizontal > 0.5 ? uv.y : uv.x;
    if (u_reverse > 0.5) along = 1.0 - along;
    float len = u_horizontal > 0.5 ? u_size.x : u_size.y;
    float wide = u_horizontal > 0.5 ? u_size.y : u_size.x;
    float p = clamp(u_progress, 0.0, 1.0);
    vec3 glow = u_glow.rgb;
    vec4 c = vec4(0.0);

    if (u_style < 0.5) {
        // Nebula: a noisy front sweeps back towards the attached edge; pixels
        // past it burn out in the accent colour and leave twinkling dust.
        vec2 px = uv * u_size;
        float n = fbm(px / 15.0 + u_seed * 13.0);
        float field = mix(n, along, 0.5);
        float t = p * 1.25 - 0.12;
        float d = t - field;
        vec4 src = texture(source, uv);
        float keep = smoothstep(0.0, 0.035, d);
        float edge = exp(-abs(d) * 40.0) * (1.0 - p * p);
        vec2 cell = floor(px / 3.0);
        float h = hash12(cell + u_seed * 91.0);
        float dust = d < 0.0 && d > -0.22 && h > 0.86 ? (1.0 + d / 0.22) * (h - 0.86) / 0.14 : 0.0;
        float spark = (edge * 0.9 + dust) * max(src.a, 0.35);
        c = src * keep;
        c.rgb += mix(glow, vec3(1.0), 0.35) * spark;
        c.a = max(c.a, spark);
    } else if (u_style < 1.5) {
        // Warp: the content is squeezed into the visible extent and smeared
        // along the axis, splitting into RGB streaks with hot light lines.
        if (along > p) { fragColor = vec4(0.0); return; }
        float s = along / max(p, 0.0001);
        float k = 1.0 - p;
        float smear = k * 0.45;
        vec4 acc = vec4(0.0);
        for (int i = 0; i < 8; i++) {
            float o = float(i) / 7.0 * smear;
            acc.r += tap(s * (1.0 + 0.05 * k) - o, across).r;
            acc.g += tap(s - o, across).g;
            acc.b += tap(s * (1.0 - 0.05 * k) - o, across).b;
            acc.a += tap(s - o, across).a;
        }
        acc /= 8.0;
        float row = floor(across * wide / 2.0);
        float h = hash12(vec2(row, u_seed * 17.0));
        float line = h > 0.9 ? (h - 0.9) * 10.0 * k * smoothstep(0.0, 0.3, along) : 0.0;
        c = acc * smoothstep(0.0, 0.2, p);
        c.rgb += mix(glow, vec3(1.0), 0.5) * line * 0.8;
        c.a = max(c.a, line * 0.8);
    } else {
        // Fold: four panels share the visible extent; alternate panels tilt
        // away (darker, pinched across) and the creases catch the light.
        if (along > p) { fragColor = vec4(0.0); return; }
        const float N = 4.0;
        float panel = p / N;
        float k = floor(along / max(panel, 0.0001));
        float f = fract(along / max(panel, 0.0001));
        float odd = mod(k, 2.0);
        float tilt = 1.0 - p;
        float depth = odd > 0.5 ? f : 1.0 - f;
        float pinch = 1.0 - 0.12 * tilt * depth;
        float a2 = 0.5 + (across - 0.5) / pinch;
        vec4 src = tap((k + f) / N, a2);
        float shade = odd > 0.5 ? 1.0 - 0.55 * tilt * (0.4 + 0.6 * f) : 1.0 - 0.25 * tilt * (1.0 - f);
        float crease = exp(-min(f, 1.0 - f) * panel * len / 2.5) * tilt;
        c = vec4(src.rgb * shade, src.a);
        c.rgb += glow * crease * 0.45 * src.a;
    }

    c.rgb = min(c.rgb, vec3(c.a));
    fragColor = c * u_opacity;
}
