#version 450

layout(location = 0) in vec2 v_local;
layout(location = 1) in vec4 v_color;
layout(location = 2) flat in vec4 v_params;
layout(location = 0) out vec4 out_color;

void main() {
    float kind = v_params.x;
    float r = length(v_local);
    float shape = 0.0;

    if (kind < 0.5) {
        // Halo: broad soft radial glow.
        shape = exp(-r * r * 3.5) * smoothstep(1.15, 0.0, r);
    } else if (kind < 1.5) {
        // Ghost: bright core with a soft outer ring.
        float core = exp(-r * r * 8.0);
        float ring = exp(-pow((r - 0.62) * 8.0, 2.0)) * 0.45;
        shape = (core + ring) * smoothstep(1.1, 0.0, r);
    } else {
        // Streak: anamorphic horizontal beam plus a small core.
        float beam = exp(-abs(v_local.y) * 18.0)
            * exp(-abs(v_local.x) * 1.6);
        float core = exp(-r * r * 12.0);
        shape = max(beam, core);
    }

    float alpha = clamp(v_color.a * shape, 0.0, 1.0);
    out_color = vec4(v_color.rgb * alpha, alpha);
}
