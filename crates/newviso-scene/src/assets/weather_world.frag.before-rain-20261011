#version 450

layout(set = 0, binding = 1) uniform texture2D t_diffuse;
layout(set = 0, binding = 2) uniform texture2D t_distortion;
layout(set = 0, binding = 3) uniform texture2D t_splash;
layout(set = 0, binding = 4) uniform sampler s_weather;

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec4 v_color;
layout(location = 2) in float v_life;
layout(location = 3) in float v_distance_fade;
layout(location = 0) out vec4 out_color;

void main() {
    vec2 distortion = texture(sampler2D(t_distortion, s_weather), v_uv).rg * 2.0 - 1.0;
    vec2 uv = v_uv + distortion * 0.006;
    vec4 diffuse = texture(sampler2D(t_diffuse, s_weather), uv);
    vec4 splash = texture(sampler2D(t_splash, s_weather), uv);

    // Source GPU FX sometimes use a dedicated splash sheet and sometimes
    // point it at the same texture. Preserve both without requiring a shader
    // permutation for every original WeatherGpuFx entry.
    vec4 texel = max(diffuse, splash * 0.35);
    float life_fade = smoothstep(0.0, 0.06, v_life)
        * (1.0 - smoothstep(0.82, 1.0, v_life));
    float alpha = texel.a * v_color.a * v_distance_fade * life_fade;
    if (alpha <= 0.001) {
        discard;
    }

    out_color = vec4(texel.rgb * v_color.rgb, alpha);
}
