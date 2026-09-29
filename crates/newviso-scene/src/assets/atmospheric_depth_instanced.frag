#version 450

layout(set = 1, binding = 0) uniform texture2D t_base_color;
layout(set = 1, binding = 5) uniform sampler s_material;
layout(set = 1, binding = 6, std140) uniform MaterialParams {
    vec4 shading0;
    vec4 shading1;
    vec4 shading2;
} material;

layout(location = 0) in float v_linear_depth;
layout(location = 1) in vec2 v_uv;
layout(location = 2) in float v_vertex_alpha;
layout(location = 0) out float out_linear_depth;

const int MATERIAL_ALPHA_TEST = 8;
const int MATERIAL_USE_VERTEX_COLOR = 64;

void main() {
    // Match the visible surface coverage. Blended surfaces never enter this pass.
    int flags = int(material.shading1.z + 0.5);
    if ((flags & MATERIAL_ALPHA_TEST) != 0) {
        float alpha = texture(sampler2D(t_base_color, s_material), v_uv).a
            * clamp(material.shading1.w, 0.0, 1.0);
        if ((flags & MATERIAL_USE_VERTEX_COLOR) != 0) {
            alpha *= v_vertex_alpha;
        }
        if (alpha < material.shading1.y) {
            discard;
        }
    }
    out_linear_depth = v_linear_depth;
}
