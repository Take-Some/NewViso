#version 450

layout(set = 0, binding = 1) uniform texture2D t_base_noise;
layout(set = 0, binding = 2) uniform texture2D t_starfield;
layout(set = 0, binding = 3) uniform texture2D t_detail_noise;
layout(set = 0, binding = 4) uniform sampler s_sky;

layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 out_color;

void main() {
    vec4 cloud_noise = texture(sampler2D(t_base_noise, s_sky), v_uv);
    vec4 stars = texture(sampler2D(t_starfield, s_sky), v_uv);
    float detail = texture(sampler2D(t_detail_noise, s_sky), v_uv * 2.0).r;

    vec3 sky_base = vec3(0.38, 0.55, 0.90);
    vec3 emissive_tint = vec3(0.35, 0.55, 0.90);
    float cloud = clamp(cloud_noise.r * 0.70 + detail * 0.30, 0.0, 1.0);
    vec3 cloud_color = mix(sky_base * 0.72, sky_base * 1.18, cloud);
    vec3 star_color = stars.rgb * 2.6 * emissive_tint;

    out_color = vec4(cloud_color + star_color, 1.0);
}
