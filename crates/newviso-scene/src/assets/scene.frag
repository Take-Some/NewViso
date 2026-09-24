#version 450

layout(set = 0, binding = 0, std140) uniform SceneFrame {
    mat4 view_proj;
    mat4 shadow_view_proj;
    vec4 globals;
    vec4 shadow_params;
    vec4 light_meta[4];
    vec4 light_pos[4];
    vec4 light_dir[4];
    vec4 light_color[4];
    vec4 light_cone[4];
    vec4 camera_position;
    vec4 environment_ambient;
    vec4 environment_fog_color_density;
    vec4 environment_fog_params;
    vec4 environment_haze_color_density;
    vec4 environment_haze_params;
} frame;

layout(set = 0, binding = 1) uniform texture2D t_shadow;
layout(set = 0, binding = 2) uniform sampler s_shadow;

layout(location = 0) in vec4 v_color;
layout(location = 1) in vec3 v_normal;
layout(location = 2) in vec3 v_world_position;
layout(location = 3) in vec4 v_shadow_coord;
layout(location = 4) flat in float v_overlay;
layout(location = 0) out vec4 out_color;

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

void main() {
    if (v_overlay > 0.5) {
        out_color = v_color;
        return;
    }

    vec3 n = normalize(v_normal);
    vec3 lighting =
        max(frame.environment_ambient.rgb, vec3(0.0))
        * max(frame.environment_ambient.a, 0.0);
    int light_count = clamp(int(frame.globals.x + 0.5), 0, 4);
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
            attenuation = (1.0 - normalized_distance);
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

        float shadow = 0.0;
        if (i == shadow_light_index) {
            shadow = sample_shadow(n, n_dot_l);
        }
        lighting += color * intensity * attenuation * n_dot_l * (1.0 - shadow);
    }

    vec3 surface_color = v_color.rgb * lighting;
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

    out_color = vec4(surface_color, v_color.a);
}
