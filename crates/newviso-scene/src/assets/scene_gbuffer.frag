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

layout(set = 0, binding = 1) uniform texture2D t_shadow;
layout(set = 0, binding = 2) uniform sampler s_shadow;

layout(set = 1, binding = 0) uniform texture2D t_base_color;
layout(set = 1, binding = 1) uniform texture2D t_normal;
layout(set = 1, binding = 2) uniform texture2D t_specular;
layout(set = 1, binding = 3) uniform texture2D t_emissive;
layout(set = 1, binding = 4) uniform texture2D t_environment;
layout(set = 1, binding = 5) uniform sampler s_material;
layout(set = 1, binding = 6, std140) uniform MaterialParams {
    vec4 shading0; // x normal strength, y spec intensity, z spec falloff, w fresnel
    vec4 shading1; // x emissive multiplier, y alpha cutoff, z flags, w opacity
    vec4 shading2; // x environment reflection strength
} material;

layout(set = 2, binding = 0) uniform texture2D t_weather_puddle_layout;
layout(set = 2, binding = 1) uniform texture2D t_weather_puddle_normal;
layout(set = 2, binding = 2) uniform sampler s_weather;
layout(set = 2, binding = 3, std140) uniform WeatherMaterial {
    vec4 state0; // x rain, y accumulated wetness, z lightning flash, w time
    vec4 state1; // x ripple scale, y ripple bumpiness, z wind speed, w puddle frame
} weather;

layout(location = 0) in vec4 v_color;
layout(location = 1) in vec3 v_normal;
layout(location = 2) in vec3 v_world_position;
layout(location = 3) in vec4 v_shadow_coord;
layout(location = 4) flat in float v_overlay;
layout(location = 5) in vec2 v_uv;
layout(location = 6) in vec4 v_tangent;

layout(location = 0) out vec4 out_albedo_occlusion;
layout(location = 1) out vec4 out_normal_twiddle;
layout(location = 2) out vec4 out_material_shadow;

const int MATERIAL_HAS_NORMAL = 1;
const int MATERIAL_HAS_SPECULAR = 2;
const int MATERIAL_HAS_EMISSIVE = 4;
const int MATERIAL_ALPHA_TEST = 8;
const int MATERIAL_ALPHA_BLEND = 16;
const int MATERIAL_ENVIRONMENT_REFLECTION = 32;
const int MATERIAL_USE_VERTEX_COLOR = 64;

float sample_shadow(vec3 normal, float n_dot_l) {
    if (frame.globals.w < 0.5) {
        return 0.0;
    }

    float normal_scale = frame.shadow_params.y * (1.0 - n_dot_l);
    vec3 receiver_position = v_world_position + normalize(normal) * normal_scale;
    vec4 shadow_coord = frame.shadow_view_proj * vec4(receiver_position, 1.0);
    if (abs(shadow_coord.w) < 1.0e-6) {
        return 0.0;
    }

    vec3 ndc = shadow_coord.xyz / shadow_coord.w;
    vec2 uv = ndc.xy * 0.5 + 0.5;
    float depth = ndc.z;
    if (uv.x <= 0.0 || uv.x >= 1.0 || uv.y <= 0.0 || uv.y >= 1.0
        || depth <= 0.0 || depth >= 1.0) {
        return 0.0;
    }

    float bias = max(frame.shadow_params.x, 0.000001);
    float inv_resolution = 1.0 / max(frame.shadow_params.z, 1.0);
    float occluded = 0.0;
    for (int y = -1; y <= 1; ++y) {
        for (int x = -1; x <= 1; ++x) {
            float stored = texture(
                sampler2D(t_shadow, s_shadow),
                uv + vec2(x, y) * inv_resolution
            ).r;
            occluded += (depth - bias > stored) ? 1.0 : 0.0;
        }
    }
    return occluded / 9.0;
}

vec3 surface_normal(int flags) {
    vec3 n = normalize(v_normal);
    if ((flags & MATERIAL_HAS_NORMAL) == 0) {
        return n;
    }

    vec3 tangent = v_tangent.xyz - n * dot(v_tangent.xyz, n);
    if (dot(tangent, tangent) < 1.0e-8) {
        vec3 axis = abs(n.y) < 0.999
            ? vec3(0.0, 1.0, 0.0)
            : vec3(1.0, 0.0, 0.0);
        tangent = cross(axis, n);
    }
    tangent = normalize(tangent);
    vec3 bitangent =
        normalize(cross(n, tangent)) * (v_tangent.w < 0.0 ? -1.0 : 1.0);

    vec3 encoded = texture(sampler2D(t_normal, s_material), v_uv).rgb;
    vec3 tangent_normal;
    if (encoded.b < 0.01) {
        vec2 xy = encoded.rg * 2.0 - 1.0;
        tangent_normal = vec3(xy, sqrt(max(1.0 - dot(xy, xy), 0.0)));
    } else {
        tangent_normal = encoded * 2.0 - 1.0;
    }
    tangent_normal.xy *= max(material.shading0.x, 0.0);
    tangent_normal = normalize(tangent_normal);
    return normalize(mat3(tangent, bitangent, n) * tangent_normal);
}

void main() {
    if (v_overlay > 0.5) {
        vec3 n = normalize(v_normal);
        out_albedo_occlusion = vec4(max(v_color.rgb, vec3(0.0)), 1.0);
        out_normal_twiddle = vec4(n * 0.5 + 0.5, 1.0);
        out_material_shadow = vec4(0.0, 1.0, 0.0, 1.0);
        return;
    }

    int flags = int(material.shading1.z + 0.5);
    vec4 base_color = texture(sampler2D(t_base_color, s_material), v_uv);
    if ((flags & MATERIAL_USE_VERTEX_COLOR) != 0) {
        base_color *= v_color;
    }
    base_color.a *= clamp(material.shading1.w, 0.0, 1.0);

    if ((flags & MATERIAL_ALPHA_TEST) != 0 && base_color.a < material.shading1.y) {
        discard;
    }
    // Alpha-blended geometry belongs to the transparent forward pass, never the GBuffer.
    if ((flags & MATERIAL_ALPHA_BLEND) != 0) {
        discard;
    }

    vec3 n = surface_normal(flags);

    float rain_amount = clamp(weather.state0.x, 0.0, 1.0);
    float wetness = clamp(weather.state0.y, 0.0, 1.0);
    float wet_surface = 0.0;
    if (wetness > 0.0) {
        float horizontal = smoothstep(0.28, 0.86, max(n.y, 0.0));
        vec2 weather_uv = v_world_position.xz * max(weather.state1.x, 0.001);
        float puddle_layout = texture(
            sampler2D(t_weather_puddle_layout, s_weather),
            weather_uv
        ).r;
        vec3 puddle_encoded = texture(
            sampler2D(t_weather_puddle_normal, s_weather),
            weather_uv * 1.75
        ).rgb;
        vec2 puddle_xy = puddle_encoded.rg * 2.0 - 1.0;
        float puddle_z = sqrt(max(1.0 - dot(puddle_xy, puddle_xy), 0.0));
        vec3 puddle_normal = normalize(vec3(puddle_xy.x, puddle_z, puddle_xy.y));
        float ripple_strength = clamp(weather.state1.y, 0.0, 2.0);
        puddle_normal = normalize(mix(
            vec3(0.0, 1.0, 0.0),
            puddle_normal,
            clamp(ripple_strength, 0.0, 1.0)
        ));
        wet_surface = wetness
            * horizontal
            * mix(0.32, 1.0, clamp(puddle_layout, 0.0, 1.0));
        n = normalize(mix(n, puddle_normal, wet_surface * 0.48));
    }
    base_color.rgb *= mix(1.0, 0.72, wet_surface);

    vec3 specular_map = (flags & MATERIAL_HAS_SPECULAR) != 0
        ? texture(sampler2D(t_specular, s_material), v_uv).rgb
        : vec3(1.0);
    float specular_intensity =
        max(material.shading0.y, 0.0) * dot(specular_map, vec3(0.3333333))
        + wet_surface * (0.9 + rain_amount * 1.4);
    float specular_falloff = mix(
        clamp(material.shading0.z, 1.0, 512.0),
        112.0,
        wet_surface
    );
    float roughness = clamp(sqrt(2.0 / (specular_falloff + 2.0)), 0.035, 1.0);
    roughness = mix(roughness, 0.055, wet_surface);

    float diffuse_spec_mix = clamp(
        specular_intensity / (1.0 + specular_intensity),
        0.0,
        1.0
    );
    float metallic_fresnel = clamp(
        max(material.shading0.w, material.shading2.x * 0.5),
        0.0,
        1.0
    );

    int light_count = clamp(int(frame.globals.x + 0.5), 0, 16);
    int shadow_light_index = int(frame.globals.z + 0.5);
    float shadow_visibility = 1.0;
    if (shadow_light_index >= 0 && shadow_light_index < light_count) {
        vec3 light_dir = normalize(-frame.light_dir[shadow_light_index].xyz);
        float n_dot_l = max(dot(n, light_dir), 0.0);
        shadow_visibility = 1.0 - sample_shadow(n, n_dot_l);
    }

    out_albedo_occlusion = vec4(max(base_color.rgb, vec3(0.0)), 1.0);
    out_normal_twiddle = vec4(n * 0.5 + 0.5, 1.0);
    out_material_shadow = vec4(
        diffuse_spec_mix,
        roughness,
        metallic_fresnel,
        clamp(shadow_visibility, 0.0, 1.0)
    );
}
