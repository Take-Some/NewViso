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
    vec4 camera_position;
    vec4 environment_ambient;
    vec4 environment_fog_color_density;
    vec4 environment_fog_params;
    vec4 environment_haze_color_density;
    vec4 environment_haze_params;
    vec4 environment_clear_color;
} frame;

layout(location = 0) in vec4 in_position_mode;
layout(location = 1) in vec3 in_normal;
layout(location = 2) in vec4 in_color;
layout(location = 3) in vec2 in_uv;
layout(location = 4) in vec4 in_tangent;

layout(location = 5) in vec4 instance_col0;
layout(location = 6) in vec4 instance_col1;
layout(location = 7) in vec4 instance_col2;
layout(location = 8) in vec4 instance_col3;

layout(location = 0) out vec4 v_color;
layout(location = 1) out vec3 v_normal;
layout(location = 2) out vec3 v_world_position;
layout(location = 3) out vec4 v_shadow_coord;
layout(location = 4) flat out float v_overlay;
layout(location = 5) out vec2 v_uv;
layout(location = 6) out vec4 v_tangent;

void main() {
    mat4 model = mat4(instance_col0, instance_col1, instance_col2, instance_col3);
    mat3 linear = mat3(model);
    mat3 normal_matrix = transpose(inverse(linear));

    vec4 world = model * vec4(in_position_mode.xyz, 1.0);
    vec3 world_normal = normalize(normal_matrix * in_normal);
    vec3 world_tangent = normalize(linear * in_tangent.xyz);
    float orientation = determinant(linear) < 0.0 ? -1.0 : 1.0;

    v_color = in_color;
    v_overlay = 0.0;
    v_uv = in_uv;
    v_tangent = vec4(world_tangent, in_tangent.w * orientation);
    v_normal = world_normal;
    v_world_position = world.xyz;
    v_shadow_coord = frame.shadow_view_proj * world;
    gl_Position = frame.view_proj * world;
}
