#version 450

layout(set = 1, binding = 0) uniform texture2D t_base_color;
layout(set = 1, binding = 5) uniform sampler s_material;
layout(set = 1, binding = 6, std140) uniform MaterialParams {
    vec4 shading0;
    vec4 shading1; // x emissive, y alpha cutoff, z flags, w bucket
} material;

layout(location = 0) in vec2 v_uv;
layout(location = 0) out float out_depth;

const int MATERIAL_ALPHA_TEST = 8;
const int MATERIAL_ALPHA_BLEND = 16;

void main() {
    int flags = int(material.shading1.z + 0.5);
    if ((flags & (MATERIAL_ALPHA_TEST | MATERIAL_ALPHA_BLEND)) != 0) {
        float alpha = texture(sampler2D(t_base_color, s_material), v_uv).a
            * clamp(material.shading1.w, 0.0, 1.0);
        float cutoff = (flags & MATERIAL_ALPHA_TEST) != 0
            ? material.shading1.y
            : min(material.shading1.y, 0.15);
        if (alpha < cutoff) {
            discard;
        }
    }
    out_depth = gl_FragCoord.z;
}
