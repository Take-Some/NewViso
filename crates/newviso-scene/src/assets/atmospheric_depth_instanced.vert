#version 450

layout(set = 0, binding = 0, std140) uniform AtmosphericDepthFrame {
    mat4 view_proj;
    vec4 camera_far;
} depth_frame;

layout(location = 0) in vec4 in_position_mode;
layout(location = 2) in vec4 in_color;
layout(location = 3) in vec2 in_uv;
layout(location = 5) in vec4 instance_col0;
layout(location = 6) in vec4 instance_col1;
layout(location = 7) in vec4 instance_col2;
layout(location = 8) in vec4 instance_col3;

layout(location = 0) out float v_linear_depth;
layout(location = 1) out vec2 v_uv;
layout(location = 2) out float v_vertex_alpha;

void main() {
    mat4 model = mat4(instance_col0, instance_col1, instance_col2, instance_col3);
    vec3 world = (model * vec4(in_position_mode.xyz, 1.0)).xyz;
    gl_Position = depth_frame.view_proj * vec4(world, 1.0);
    v_linear_depth = max(gl_Position.w, 0.0);
    v_uv = in_uv;
    v_vertex_alpha = in_color.a;
}
