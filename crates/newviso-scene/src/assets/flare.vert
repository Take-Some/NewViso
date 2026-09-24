#version 450

layout(location = 0) in vec4 in_position_local;
layout(location = 1) in vec4 in_color;
layout(location = 2) in vec4 in_params;

layout(location = 0) out vec2 v_local;
layout(location = 1) out vec4 v_color;
layout(location = 2) flat out vec4 v_params;

void main() {
    gl_Position = vec4(in_position_local.xy, 0.0, 1.0);
    v_local = in_position_local.zw;
    v_color = in_color;
    v_params = in_params;
}
