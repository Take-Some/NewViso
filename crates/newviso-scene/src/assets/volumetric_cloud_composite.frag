#version 450

layout(set = 0, binding = 0) uniform texture2D t_cloud_history;
layout(set = 0, binding = 1) uniform sampler s_cloud;

layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 out_color;

void main() {
    vec4 cloud = texture(sampler2D(t_cloud_history, s_cloud), clamp(v_uv, 0.0, 1.0));
    out_color = vec4(max(cloud.rgb, vec3(0.0)), clamp(cloud.a, 0.0, 1.0));
}
