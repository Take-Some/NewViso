#version 450

layout(set = 0, binding = 0, std140) uniform SceneFrame {
    mat4 view_proj;
    mat4 shadow_view_proj;
    vec4 globals;
    vec4 shadow_params;
    vec4 light_meta[4];
    vec4 light_pos[4];
    vec4 light_dir[4];
    vec4 light_color[4];
    vec4 light_cone[4];
} frame;

layout(location = 0) in vec4 in_position_mode;

void main() {
    gl_Position = frame.shadow_view_proj * vec4(in_position_mode.xyz, 1.0);
}
