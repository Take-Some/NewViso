#version 450

layout(location = 0) in vec4 in_clip_position;
layout(location = 1) in vec3 in_color;

layout(location = 0) out vec3 v_color;

void main() {
    gl_Position = in_clip_position;
    v_color = in_color;
}
