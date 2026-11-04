#version 300 es
precision highp float;
// Port of omarchy-t3-code. Copyright (c) 2026 Andrés Villagrán. MIT; see LICENSE.
in vec2 v_uv;
layout(location = 0) out vec4 fragColor;
uniform float u_opacity;
uniform float u_progress;
uniform float u_textWidth;
uniform float u_bandWidth;
uniform sampler2D source;
void main() {
    float x = (v_uv.x * u_textWidth + u_bandWidth
             - u_progress * (u_textWidth + 2.0 * u_bandWidth)) / u_bandWidth;
    float d = abs(x - 0.5);
    // Gaussian falloff: the same peak and half-brightness width as the CSS
    // stops, without their visible kinks.
    float mask = exp(-(d * d) / 0.04);
    fragColor = texture(source, v_uv) * mask * u_opacity;
}
