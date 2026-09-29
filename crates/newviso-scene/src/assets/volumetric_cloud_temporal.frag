#version 450

layout(set = 0, binding = 0, std140) uniform VolumetricCloudFrame {
    vec4 camera_position_time;
    vec4 camera_right_tan_x;
    vec4 camera_up_tan_y;
    vec4 camera_forward_frame;
    vec4 volume_bounds;
    vec4 shape_params;
    vec4 detail_params;
    vec4 light_params;
    vec4 sun_direction_intensity;
    vec4 cloud_day_shadow;
    vec4 cloud_day_light;
    vec4 cloud_night;
    vec4 motion;
    vec4 viewport;
    vec4 temporal_params;
    mat4 previous_view_proj;
} cloud;

layout(set = 0, binding = 1) uniform texture2D t_current;
layout(set = 0, binding = 2) uniform texture2D t_history;
layout(set = 0, binding = 3) uniform sampler s_cloud;

layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 out_color;

vec3 reconstruct_ray(vec2 uv) {
    vec2 ndc = uv * 2.0 - 1.0;
    return normalize(
        cloud.camera_forward_frame.xyz
        + cloud.camera_right_tan_x.xyz * ndc.x * cloud.camera_right_tan_x.w
        - cloud.camera_up_tan_y.xyz * ndc.y * cloud.camera_up_tan_y.w
    );
}

vec2 reproject_uv(vec2 uv, out float valid) {
    vec3 ray = reconstruct_ray(uv);
    float mid_altitude = cloud.temporal_params.z;
    float max_distance = cloud.volume_bounds.z;
    float t = max_distance * 0.35;
    if (abs(ray.y) > 1e-5) {
        float candidate = (mid_altitude - cloud.camera_position_time.y) / ray.y;
        if (candidate > 0.0) {
            t = min(candidate, max_distance);
        }
    }
    vec3 world = cloud.camera_position_time.xyz + ray * t;
    vec4 previous_clip = cloud.previous_view_proj * vec4(world, 1.0);
    if (previous_clip.w <= 1e-5) {
        valid = 0.0;
        return uv;
    }
    vec2 ndc = previous_clip.xy / previous_clip.w;
    vec2 previous_uv = ndc * 0.5 + 0.5;
    valid = float(
        previous_uv.x >= 0.0 && previous_uv.x <= 1.0
        && previous_uv.y >= 0.0 && previous_uv.y <= 1.0
    );
    return clamp(previous_uv, vec2(0.0), vec2(1.0));
}

void main() {
    vec4 current = texture(sampler2D(t_current, s_cloud), v_uv);
    if (cloud.temporal_params.y < 0.5) {
        out_color = current;
        return;
    }

    float valid;
    vec2 history_uv = reproject_uv(v_uv, valid);
    vec4 history = texture(sampler2D(t_history, s_cloud), history_uv);

    vec2 texel = 1.0 / max(cloud.viewport.xy, vec2(1.0));
    vec4 c0 = texture(sampler2D(t_current, s_cloud), clamp(v_uv + vec2(texel.x, 0.0), 0.0, 1.0));
    vec4 c1 = texture(sampler2D(t_current, s_cloud), clamp(v_uv - vec2(texel.x, 0.0), 0.0, 1.0));
    vec4 c2 = texture(sampler2D(t_current, s_cloud), clamp(v_uv + vec2(0.0, texel.y), 0.0, 1.0));
    vec4 c3 = texture(sampler2D(t_current, s_cloud), clamp(v_uv - vec2(0.0, texel.y), 0.0, 1.0));

    vec4 local_min = min(current, min(min(c0, c1), min(c2, c3)));
    vec4 local_max = max(current, max(max(c0, c1), max(c2, c3)));
    history = clamp(history, local_min, local_max);

    float alpha_disagreement = abs(history.a - current.a);
    float history_weight = clamp(cloud.temporal_params.x, 0.0, 0.98)
        * valid
        * exp(-alpha_disagreement * 7.0);
    out_color = mix(current, history, history_weight);
}
