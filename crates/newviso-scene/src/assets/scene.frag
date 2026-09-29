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
    vec4 shading0; // x bumpiness, y spec intensity, z spec falloff, w spec fresnel
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
layout(location = 0) out vec4 out_color;

const int MATERIAL_HAS_NORMAL = 1;
const int MATERIAL_HAS_SPECULAR = 2;
const int MATERIAL_HAS_EMISSIVE = 4;
const int MATERIAL_ALPHA_TEST = 8;
const int MATERIAL_ALPHA_BLEND = 16;
const int MATERIAL_ENVIRONMENT_REFLECTION = 32;
const int MATERIAL_USE_VERTEX_COLOR = 64;
const int MATERIAL_HAS_ENVIRONMENT_TEXTURE = 128;

float sample_shadow(vec3 normal, float n_dot_l) {
    if (frame.globals.w < 0.5) {
        return 0.0;
    }

    float normal_scale = frame.shadow_params.y * (1.0 - n_dot_l);
    vec3 receiver_position = v_world_position + normalize(normal) * normal_scale;
    vec4 shadow_coord = frame.shadow_view_proj * vec4(receiver_position, 1.0);
    if (abs(shadow_coord.w) < 1e-6) {
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

vec3 rsc7_surface_normal(int flags) {
    vec3 n = normalize(v_normal);
    if ((flags & MATERIAL_HAS_NORMAL) == 0) {
        return n;
    }

    vec3 tangent = v_tangent.xyz - n * dot(v_tangent.xyz, n);
    if (dot(tangent, tangent) < 1e-8) {
        vec3 axis = abs(n.y) < 0.999 ? vec3(0.0, 1.0, 0.0) : vec3(1.0, 0.0, 0.0);
        tangent = cross(axis, n);
    }
    tangent = normalize(tangent);
    vec3 bitangent = normalize(cross(n, tangent)) * (v_tangent.w < 0.0 ? -1.0 : 1.0);

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
        out_color = v_color;
        return;
    }

    int flags = int(material.shading1.z + 0.5);
    vec4 base_texel = texture(sampler2D(t_base_color, s_material), v_uv);
    // COLOR0 is not implicitly an albedo tint. Imported RSC7 shaders often
    // use vertex-colour channels as masks/AO/auxiliary data; direct colour
    // modulation is therefore an explicit material capability.
    vec4 base_color = base_texel;
    if ((flags & MATERIAL_USE_VERTEX_COLOR) != 0) {
        base_color *= v_color;
    }
    base_color.a *= clamp(material.shading1.w, 0.0, 1.0);

    if ((flags & MATERIAL_ALPHA_TEST) != 0 && base_color.a < material.shading1.y) {
        discard;
    }
    if ((flags & MATERIAL_ALPHA_BLEND) != 0 && base_color.a <= 0.002) {
        discard;
    }

    vec3 n = rsc7_surface_normal(flags);

    // Weather wetness is global renderer state rather than a material flag.
    // Horizontal authored surfaces progressively darken, gain a moving GTA
    // puddle normal and become more specular while vertical walls remain
    // largely unchanged.
    float rain_amount = clamp(weather.state0.x, 0.0, 1.0);
    float accumulated_wetness = clamp(weather.state0.y, 0.0, 1.0);
    float wet_surface = 0.0;
    // Uniform across the draw, so implicit texture derivatives remain valid.
    // Dry weather preserves the original normal without two texture fetches.
    if (accumulated_wetness > 0.0) {
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
        wet_surface = accumulated_wetness
            * horizontal
            * mix(0.32, 1.0, clamp(puddle_layout, 0.0, 1.0));
        n = normalize(mix(n, puddle_normal, wet_surface * 0.48));
    }
    base_color.rgb *= mix(1.0, 0.72, wet_surface);

    vec3 v = normalize(frame.camera_position.xyz - v_world_position);
    vec3 diffuse_lighting =
        max(frame.environment_ambient.rgb, vec3(0.0))
        * max(frame.environment_ambient.a, 0.0);
    vec3 specular_lighting = vec3(0.0);

    vec3 specular_map = (flags & MATERIAL_HAS_SPECULAR) != 0
        ? texture(sampler2D(t_specular, s_material), v_uv).rgb
        : vec3(1.0);
    float specular_intensity = max(material.shading0.y, 0.0)
        + wet_surface * (0.9 + rain_amount * 1.4);
    float specular_falloff = mix(
        clamp(material.shading0.z, 1.0, 512.0),
        112.0,
        wet_surface
    );
    float authored_fresnel = clamp(material.shading0.w, 0.0, 1.0);
    float view_fresnel = pow(1.0 - max(dot(n, v), 0.0), 5.0);
    float fresnel_gain = mix(1.0, 0.25 + 0.75 * view_fresnel, authored_fresnel);

    int light_count = clamp(int(frame.globals.x + 0.5), 0, 16);
    int shadow_light_index = int(frame.globals.z + 0.5);

    for (int i = 0; i < light_count; ++i) {
        int light_type = int(frame.light_meta[i].x + 0.5);
        float intensity = max(frame.light_meta[i].y, 0.0);
        float range = max(frame.light_meta[i].z, 0.001);
        vec3 color = max(frame.light_color[i].rgb, vec3(0.0));
        vec3 l = vec3(0.0);
        float attenuation = 1.0;

        if (light_type == 0) {
            l = normalize(-frame.light_dir[i].xyz);
        } else {
            vec3 to_light = frame.light_pos[i].xyz - v_world_position;
            float distance_to_light = length(to_light);
            if (distance_to_light <= 1e-5 || distance_to_light >= range) {
                continue;
            }
            l = to_light / distance_to_light;
            float normalized_distance = distance_to_light / range;
            attenuation = 1.0 - normalized_distance;
            attenuation *= attenuation;

            if (light_type == 2) {
                vec3 from_light = -l;
                float cone_cos = dot(normalize(frame.light_dir[i].xyz), from_light);
                float inner_cos = frame.light_cone[i].x;
                float outer_cos = frame.light_cone[i].y;
                attenuation *= smoothstep(outer_cos, inner_cos, cone_cos);
            } else if (light_type == 3) {
                attenuation *= 0.75;
            }
        }

        float n_dot_l = max(dot(n, l), 0.0);
        if (n_dot_l <= 0.0) {
            continue;
        }

        float shadow = i == shadow_light_index ? sample_shadow(n, n_dot_l) : 0.0;
        float visibility = 1.0 - shadow;
        vec3 radiance = color * intensity * attenuation * visibility;
        diffuse_lighting += radiance * n_dot_l;

        if (specular_intensity > 0.0) {
            vec3 h = normalize(l + v);
            float n_dot_h = max(dot(n, h), 0.0);
            float specular_power = pow(n_dot_h, specular_falloff);
            specular_lighting += radiance
                * specular_map
                * specular_intensity
                * specular_power
                * fresnel_gain;
        }
    }

    vec3 emissive = vec3(0.0);
    if ((flags & MATERIAL_HAS_EMISSIVE) != 0) {
        emissive = texture(sampler2D(t_emissive, s_material), v_uv).rgb
            * max(material.shading1.x, 0.0);
    }

    vec3 environment_reflection = vec3(0.0);
    if ((flags & MATERIAL_ENVIRONMENT_REFLECTION) != 0) {
        vec3 reflected = normalize(reflect(-v, n));
        const float PI = 3.14159265359;
        vec2 env_uv = vec2(
            atan(reflected.z, reflected.x) / (2.0 * PI) + 0.5,
            asin(clamp(reflected.y, -1.0, 1.0)) / PI + 0.5
        );
        vec3 environment_color;
        if ((flags & MATERIAL_HAS_ENVIRONMENT_TEXTURE) != 0) {
            environment_color = texture(sampler2D(t_environment, s_material), env_uv).rgb;
        } else {
            // RSC7 glass_env/glass_pv_env can reference the global scene
            // environment without a material-local sampler. Approximate that
            // source-neutrally from the active NewViso environment instead of
            // sampling the intentionally black missing-texture fallback.
            float sky_factor = clamp(reflected.y * 0.5 + 0.5, 0.0, 1.0);
            vec3 ambient_env = max(
                frame.environment_ambient.rgb * max(frame.environment_ambient.a, 0.0),
                vec3(0.0)
            );
            vec3 background_env = max(frame.environment_clear_color.rgb, vec3(0.0));
            vec3 atmospheric_env = max(
                mix(
                    frame.environment_fog_color_density.rgb,
                    frame.environment_haze_color_density.rgb,
                    sky_factor
                ),
                background_env
            );
            environment_color = max(
                mix(background_env, atmospheric_env, 0.65),
                ambient_env
            );
        }
        float facing = max(dot(n, v), 0.0);
        float glass_fresnel = 0.08 + 0.92 * pow(1.0 - facing, 5.0);
        float reflection_gain =
            max(material.shading0.y, 0.20)
            * max(material.shading2.x, 0.0)
            * mix(0.45, 1.0, 1.0 - base_color.a);
        environment_reflection =
            environment_color * glass_fresnel * reflection_gain;
    }

    float wet_view_fresnel = pow(1.0 - max(dot(n, v), 0.0), 5.0);
    vec3 wet_environment = max(
        mix(
            frame.environment_clear_color.rgb,
            frame.environment_haze_color_density.rgb,
            0.45
        ),
        vec3(0.0)
    ) * wet_surface * (0.10 + 0.42 * wet_view_fresnel);
    vec3 lightning_ambient = vec3(0.92, 0.96, 1.0)
        * clamp(weather.state0.z, 0.0, 1.0)
        * 1.35;

    vec3 surface_color =
        base_color.rgb * (diffuse_lighting + lightning_ambient)
        + specular_lighting
        + environment_reflection
        + wet_environment
        + emissive;
    float camera_distance = length(v_world_position - frame.camera_position.xyz);

    float haze_distance = max(
        camera_distance - max(frame.environment_haze_params.x, 0.0),
        0.0
    );
    float haze = 1.0 - exp(
        -max(frame.environment_haze_color_density.a, 0.0) * haze_distance
    );
    surface_color = mix(
        surface_color,
        max(frame.environment_haze_color_density.rgb, vec3(0.0)),
        clamp(haze, 0.0, 1.0)
    );

    float fog_distance = max(
        camera_distance - max(frame.environment_fog_params.x, 0.0),
        0.0
    );
    float height_above_base = max(
        v_world_position.y - frame.environment_fog_params.z,
        0.0
    );
    float height_factor = exp(
        -height_above_base * max(frame.environment_fog_params.y, 0.0)
    );
    float fog = 1.0 - exp(
        -max(frame.environment_fog_color_density.a, 0.0)
        * fog_distance
        * height_factor
    );
    fog = min(
        clamp(fog, 0.0, 1.0),
        clamp(frame.environment_fog_params.w, 0.0, 1.0)
    );
    surface_color = mix(
        surface_color,
        max(frame.environment_fog_color_density.rgb, vec3(0.0)),
        fog
    );

    out_color = vec4(surface_color, base_color.a);
}
