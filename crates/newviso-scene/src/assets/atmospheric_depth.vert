#version 450

layout(set = 0, binding = 0, std140) uniform AtmosphericDepthFrame {
    mat4 view_proj;
    vec4 camera_far;
} depth_frame;

layout(location = 0) in vec4 in_position_mode;
layout(location = 0) out float v_linear_depth;

void main() {
    vec3 world = in_position_mode.xyz;
    gl_Position = depth_frame.view_proj * vec4(world, 1.0);
    v_linear_depth = max(gl_Position.w, 0.0);
}
