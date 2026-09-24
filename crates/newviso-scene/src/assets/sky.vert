#version 450

layout(set = 0, binding = 0, std140) uniform SkyFrame {
    vec4 right;
    vec4 up;
    vec4 forward;
    vec4 projection;
    vec4 visual_globals;
    vec4 visual_dir_size[4];
    vec4 visual_color_intensity[4];
    vec4 visual_halo_kind[4];
} sky;

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec2 in_uv;
layout(location = 0) out vec2 v_uv;
layout(location = 1) out vec3 v_direction;

void main() {
    float view_x = dot(sky.right.xyz, in_position);
    float view_y = dot(sky.up.xyz, in_position);
    float view_z = dot(sky.forward.xyz, in_position);

    gl_Position = vec4(
        view_x * sky.projection.x,
        -view_y * sky.projection.y,
        view_z,
        view_z
    );
    v_uv = in_uv;
    v_direction = normalize(in_position);
}
