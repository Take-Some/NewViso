#version 450

layout(set = 0, binding = 0, std140) uniform SceneFrame {
    mat4 view_proj;
    mat4 shadow_view_proj;
    vec4 globals;
    vec4 shadow_params;
    vec4 light_meta[16];
    vec4 light_pos[16];
    vec4 light_dir[16];
    vec4 light_color[16];
    vec4 light_cone[16];
} frame;

layout(location = 0) in vec4 in_position_mode;
layout(location = 3) in vec2 in_uv;
layout(location = 0) out vec2 v_uv;

void main() {
    v_uv = in_uv;
    gl_Position = frame.shadow_view_proj * vec4(in_position_mode.xyz, 1.0);
}
