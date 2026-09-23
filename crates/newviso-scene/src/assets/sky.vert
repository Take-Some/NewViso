#version 450

layout(set = 0, binding = 0, std140) uniform SkyCamera {
    vec4 right;
    vec4 up;
    vec4 forward;
    vec4 projection;
} camera;

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec2 in_uv;
layout(location = 0) out vec2 v_uv;

void main() {
    float view_x = dot(camera.right.xyz, in_position);
    float view_y = dot(camera.up.xyz, in_position);
    float view_z = dot(camera.forward.xyz, in_position);

    vec4 clip = vec4(
        view_x * camera.projection.x,
        -view_y * camera.projection.y,
        view_z,
        view_z
    );

    gl_Position = clip;
    v_uv = in_uv;
}
