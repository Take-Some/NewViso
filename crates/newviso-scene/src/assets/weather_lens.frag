#version 450

layout(set = 0, binding = 0, std140) uniform WeatherLens {
    vec4 state0; // x rain, y lens wetness, z lightning flash, w time
    vec4 state1; // xy wind, z wind speed, w outdoor exposure
    vec4 state2; // x current/next blend, y camera motion placeholder, zw reserved
    vec4 state3;
} weather;

layout(set = 0, binding = 1) uniform texture2D t_drop;
layout(set = 0, binding = 2) uniform texture2D t_drop_normal;
layout(set = 0, binding = 3) uniform texture2D t_running_normal;
layout(set = 0, binding = 4) uniform texture2D t_lightning;
layout(set = 0, binding = 5) uniform sampler s_weather;

layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 out_color;

float hash21(vec2 p) {
    vec3 p3 = fract(vec3(p.xyx) * vec3(123.34, 456.21, 34.345));
    p3 += dot(p3, p3.yzx + 45.32);
    return fract(p3.x * p3.y * p3.z);
}

void main() {
    float wetness = clamp(weather.state0.y, 0.0, 1.0);
    float rain = clamp(weather.state0.x, 0.0, 1.0);
    float time = weather.state0.w;

    vec2 wind = weather.state1.xy;
    float wind_len = length(wind);
    if (wind_len > 1e-5) wind /= wind_len;

    // Two independently shifted copies of the original GTA lens drop atlas.
    vec2 uv0 = fract(v_uv * vec2(2.0, 1.55) + vec2(0.07, time * 0.006));
    vec2 uv1 = fract(v_uv * vec2(3.15, 2.35) + vec2(0.41, 0.19) + wind * time * 0.002);
    vec4 drop0 = texture(sampler2D(t_drop, s_weather), uv0);
    vec4 drop1 = texture(sampler2D(t_drop, s_weather), uv1);

    vec3 normal0 = texture(sampler2D(t_drop_normal, s_weather), uv0).rgb * 2.0 - 1.0;
    vec3 run_normal = texture(
        sampler2D(t_running_normal, s_weather),
        fract(v_uv + vec2(wind.x * 0.015, -time * (0.008 + rain * 0.025)))
    ).rgb * 2.0 - 1.0;

    float drop_alpha = max(drop0.a, drop1.a * 0.72) * wetness;
    float refractive_highlight = clamp(
        length(normal0.xy) * 0.16 + length(run_normal.xy) * rain * 0.10,
        0.0,
        0.22
    );

    float lightning_flash = clamp(weather.state0.z, 0.0, 1.0);
    vec2 lightning_uv = vec2(
        clamp(abs(v_uv.x - 0.5) * 1.7, 0.0, 1.0),
        fract(v_uv.y * 0.08 + time * 0.03)
    );
    float lightning_shape = texture(sampler2D(t_lightning, s_weather), lightning_uv).r;

    vec3 water_tint = vec3(0.78, 0.86, 0.92);
    vec3 color = water_tint * (drop_alpha * 0.16 + refractive_highlight);
    color += vec3(0.92, 0.96, 1.0) * lightning_flash
        * (0.22 + lightning_shape * 0.18);

    float alpha = clamp(
        drop_alpha * (0.13 + rain * 0.08)
        + refractive_highlight
        + lightning_flash * 0.26,
        0.0,
        0.48
    );
    if (alpha <= 0.001) {
        discard;
    }
    out_color = vec4(color, alpha);
}
