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

layout(set = 0, binding = 1) uniform texture2D t_base_noise;
layout(set = 0, binding = 2) uniform texture2D t_detail_noise;
layout(set = 0, binding = 3) uniform texture2D t_scene_depth;
layout(set = 0, binding = 4) uniform sampler s_cloud;

layout(location = 0) in vec2 v_uv;
layout(location = 0) out vec4 out_color;

const float PI = 3.14159265358979323846;
const int MAX_RAY_STEPS = 128;
const int MAX_LIGHT_STEPS = 24;

float remap01(float value, float lo, float hi) {
    return clamp((value - lo) / max(hi - lo, 1e-5), 0.0, 1.0);
}

float height_profile(float h, float weather) {
    // Cumulus-like profile: soft base, dense middle, increasingly broad/anvil
    // upper body. Weather slightly lifts the top instead of scaling a flat card.
    float base = smoothstep(0.0, 0.09, h);
    float top_start = mix(0.58, 0.72, weather);
    float top = 1.0 - smoothstep(top_start, 1.0, h);
    float body = base * top;
    float middle = smoothstep(0.10, 0.32, h) * (1.0 - smoothstep(0.62, 0.94, h));
    return clamp(body * mix(0.72, 1.0, middle), 0.0, 1.0);
}

float sample_density(vec3 p) {
    float base_altitude = cloud.volume_bounds.x;
    float top_altitude = cloud.volume_bounds.y;
    float h = remap01(p.y, base_altitude, top_altitude);
    if (h <= 0.0 || h >= 1.0) {
        return 0.0;
    }

    float coverage = clamp(cloud.shape_params.x, 0.0, 1.0);
    float density_scale = max(cloud.shape_params.y, 0.0);
    float shape_scale = max(cloud.shape_params.z, 1e-6);
    float detail_scale = max(cloud.shape_params.w, 1e-6);
    vec2 wind = cloud.motion.xy;

    vec2 weather_uv = p.xz * shape_scale + wind;
    vec3 weather_rgb = texture(sampler2D(t_base_noise, s_cloud), weather_uv).rgb;
    float weather_low = dot(weather_rgb, vec3(0.52, 0.31, 0.17));
    float weather_high = max(weather_rgb.r, max(weather_rgb.g, weather_rgb.b));
    float weather = mix(weather_low, weather_high, 0.58);

    float threshold = mix(0.78, 0.28, coverage);
    float macro = smoothstep(threshold - 0.15, threshold + 0.10, weather);
    if (macro <= 0.001) {
        return 0.0;
    }

    // A real XYZ-dependent density field using three orthogonal projections.
    // No carrier plane contributes to density: moving vertically changes both
    // the height profile and the detail field.
    vec2 d0_uv = p.xz * detail_scale + wind * 2.7;
    vec2 d1_uv = p.xy * detail_scale * 0.83 + wind.yx * vec2(-1.7, 1.3) + vec2(0.31, 0.73);
    vec2 d2_uv = p.zy * detail_scale * 1.11 + wind * vec2(1.9, -1.2) + vec2(-0.47, 0.19);
    float d0 = texture(sampler2D(t_detail_noise, s_cloud), d0_uv).r;
    float d1 = texture(sampler2D(t_detail_noise, s_cloud), d1_uv).r;
    float d2 = texture(sampler2D(t_detail_noise, s_cloud), d2_uv).r;
    float detail = d0 * 0.46 + d1 * 0.29 + d2 * 0.25;

    float profile = height_profile(h, weather);
    float raw = macro * profile;

    float detail_strength = clamp(cloud.detail_params.x, 0.0, 2.0);
    float erosion_strength = clamp(cloud.detail_params.y, 0.0, 2.0);
    float edge_weight = 1.0 - smoothstep(0.35, 0.82, raw);
    raw -= (1.0 - detail) * detail_strength * erosion_strength * mix(0.34, 1.0, edge_weight);

    // Small vertical modulation prevents horizontally extruded silhouettes.
    float vertical_detail = mix(d1, d2, h);
    raw += (vertical_detail - 0.5) * 0.16 * profile;

    return clamp(raw * density_scale, 0.0, 2.0);
}

float henyey_greenstein(float cos_theta, float g) {
    float gg = g * g;
    float denom = pow(max(1.0 + gg - 2.0 * g * cos_theta, 1e-4), 1.5);
    return (1.0 - gg) / (4.0 * PI * denom);
}

float light_transmittance(vec3 p, vec3 sun_dir) {
    int light_steps = clamp(int(cloud.light_params.w + 0.5), 1, MAX_LIGHT_STEPS);
    float thickness = max(cloud.volume_bounds.y - cloud.volume_bounds.x, 1.0);
    float light_distance = min(thickness * 1.35, 2400.0);
    float step_len = light_distance / float(light_steps);
    float optical_depth = 0.0;

    for (int i = 0; i < MAX_LIGHT_STEPS; ++i) {
        if (i >= light_steps) {
            break;
        }
        vec3 lp = p + sun_dir * (float(i) + 0.5) * step_len;
        if (lp.y < cloud.volume_bounds.x || lp.y > cloud.volume_bounds.y) {
            continue;
        }
        optical_depth += sample_density(lp) * step_len;
        if (optical_depth * cloud.detail_params.z > 8.0) {
            break;
        }
    }
    return exp(-optical_depth * cloud.detail_params.z);
}

vec3 reconstruct_ray(vec2 uv) {
    vec2 ndc = uv * 2.0 - 1.0;
    return normalize(
        cloud.camera_forward_frame.xyz
        + cloud.camera_right_tan_x.xyz * ndc.x * cloud.camera_right_tan_x.w
        - cloud.camera_up_tan_y.xyz * ndc.y * cloud.camera_up_tan_y.w
    );
}

bool intersect_height_slab(vec3 origin, vec3 ray, out float t0, out float t1) {
    float base_altitude = cloud.volume_bounds.x;
    float top_altitude = cloud.volume_bounds.y;
    float max_distance = cloud.volume_bounds.z;

    if (abs(ray.y) < 1e-5) {
        if (origin.y >= base_altitude && origin.y <= top_altitude) {
            t0 = 0.0;
            t1 = max_distance;
            return true;
        }
        return false;
    }

    float a = (base_altitude - origin.y) / ray.y;
    float b = (top_altitude - origin.y) / ray.y;
    t0 = max(min(a, b), 0.0);
    t1 = min(max(a, b), max_distance);
    return t1 > t0;
}

float hash12(vec2 p) {
    vec3 p3 = fract(vec3(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

void main() {
    vec3 camera = cloud.camera_position_time.xyz;
    vec3 ray = reconstruct_ray(v_uv);

    float t_enter;
    float t_exit;
    if (!intersect_height_slab(camera, ray, t_enter, t_exit)) {
        out_color = vec4(0.0);
        return;
    }

    float scene_view_depth = texture(
        sampler2D(t_scene_depth, s_cloud),
        clamp(v_uv, 0.0, 1.0)
    ).r;
    if (scene_view_depth > 0.0) {
        float ray_forward = max(
            dot(ray, normalize(cloud.camera_forward_frame.xyz)),
            1.0e-4
        );
        float scene_ray_distance = scene_view_depth / ray_forward;
        t_exit = min(t_exit, scene_ray_distance);
    }
    if (t_exit <= t_enter) {
        out_color = vec4(0.0);
        return;
    }

    int ray_steps = clamp(int(cloud.volume_bounds.w + 0.5), 8, MAX_RAY_STEPS);
    float step_len = (t_exit - t_enter) / float(ray_steps);
    float frame = cloud.camera_forward_frame.w;
    float jitter = (hash12(gl_FragCoord.xy + vec2(frame * 17.0, frame * 5.0)) - 0.5)
        * clamp(cloud.motion.z, 0.0, 2.0);

    vec3 sun_dir = normalize(cloud.sun_direction_intensity.xyz);
    float cos_theta = dot(ray, sun_dir);
    float phase = henyey_greenstein(cos_theta, clamp(cloud.light_params.y, -0.95, 0.95));
    float day_factor = smoothstep(-0.10, 0.22, cloud.cloud_day_shadow.w);
    vec3 ambient_color = mix(cloud.cloud_night.rgb, cloud.cloud_day_shadow.rgb, day_factor);
    vec3 sun_color = mix(cloud.cloud_night.rgb, cloud.cloud_day_light.rgb, day_factor);

    float transmittance = 1.0;
    vec3 radiance = vec3(0.0);
    float extinction = max(cloud.detail_params.z, 1e-5);
    float scattering = max(cloud.detail_params.w, 0.0);

    float t = t_enter + step_len * (0.5 + jitter * 0.45);
    for (int i = 0; i < MAX_RAY_STEPS; ++i) {
        if (i >= ray_steps || t >= t_exit || transmittance < 0.008) {
            break;
        }

        vec3 p = camera + ray * t;
        float density = sample_density(p);
        if (density > 0.001) {
            float optical = density * extinction * step_len;
            float step_transmittance = exp(-optical);
            float segment_alpha = 1.0 - step_transmittance;

            float sun_transmittance = light_transmittance(p, sun_dir);
            float powder = 1.0 - exp(
                -density * max(cloud.light_params.z, 0.0) * 2.0
            );
            float direct = sun_transmittance
                * (phase * 5.0 + 0.10)
                * max(cloud.sun_direction_intensity.w, 0.0)
                * scattering;
            float multi_scatter = (1.0 - sun_transmittance)
                * (0.16 + 0.34 * powder)
                * scattering;
            float ambient = max(cloud.light_params.x, 0.0)
                * mix(0.65, 1.15, remap01(p.y, cloud.volume_bounds.x, cloud.volume_bounds.y));

            vec3 sample_light =
                sun_color * (direct + multi_scatter)
                + ambient_color * ambient;
            radiance += transmittance * segment_alpha * sample_light;
            transmittance *= step_transmittance;
        }

        t += step_len;
    }

    float alpha = clamp(1.0 - transmittance, 0.0, 1.0);
    float shoulder = max(cloud.cloud_night.a, 0.0);
    radiance = radiance / (vec3(1.0) + radiance * shoulder);
    vec3 straight_color = alpha > 1.0e-5 ? radiance / alpha : vec3(0.0);
    out_color = vec4(max(straight_color, vec3(0.0)), alpha);
}
