#version 450

layout(set = 0, binding = 0, std140) uniform SkyFrame {
    vec4 right;
    vec4 up;
    vec4 forward;
    vec4 projection;
    vec4 visual_globals;
    vec4 visual_dir_size[4];
    vec4 visual_color_intensity[4];
    vec4 visual_halo_kind[4];
} sky;

layout(set = 0, binding = 1) uniform texture2D t_base_noise;
layout(set = 0, binding = 2) uniform texture2D t_starfield;
layout(set = 0, binding = 3) uniform texture2D t_detail_noise;
layout(set = 0, binding = 4) uniform texture2D t_billboard;
layout(set = 0, binding = 5) uniform sampler s_sky;

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec3 v_direction;
layout(location = 0) out vec4 out_color;

vec3 make_tangent(vec3 n) {
    vec3 axis = abs(n.y) < 0.98 ? vec3(0.0, 1.0, 0.0) : vec3(1.0, 0.0, 0.0);
    return normalize(cross(axis, n));
}

vec2 project_direction(vec3 direction) {
    vec3 forward = normalize(sky.forward.xyz);
    float view_z = dot(forward, direction);
    if (view_z <= 0.0001) {
        return vec2(1000.0);
    }

    return vec2(
        dot(normalize(sky.right.xyz), direction) * sky.projection.x / view_z,
        -dot(normalize(sky.up.xyz), direction) * sky.projection.y / view_z
    );
}

float flare_blob(vec2 point, vec2 center, float radius, float softness) {
    float distance_to_center = length(point - center) / max(radius, 0.0001);
    return exp(-distance_to_center * distance_to_center * softness)
        * (1.0 - smoothstep(0.92, 1.18, distance_to_center));
}

float smootherstep_range(float edge0, float edge1, float value) {
    float t = clamp((value - edge0) / max(edge1 - edge0, 0.00001), 0.0, 1.0);
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

void main() {
    vec3 direction = normalize(v_direction);
    int visual_count = clamp(int(sky.visual_globals.x + 0.5), 0, 4);

    vec3 sun_direction = vec3(0.0, -1.0, 0.0);
    float sun_intensity = 0.0;
    for (int i = 0; i < visual_count; ++i) {
        float kind = sky.visual_halo_kind[i].z;
        float intensity = max(sky.visual_color_intensity[i].a, 0.0);
        if (kind < 0.5 && intensity > sun_intensity) {
            sun_intensity = intensity;
            sun_direction = normalize(sky.visual_dir_size[i].xyz);
        }
    }

    float sun_elevation = clamp(sun_direction.y, -1.0, 1.0);
    float sun_altitude_degrees = degrees(asin(sun_elevation));

    float astronomical = smootherstep_range(-18.0, -12.0, sun_altitude_degrees);
    float nautical = smootherstep_range(-12.0, -6.0, sun_altitude_degrees);
    float civil = smootherstep_range(-6.0, 4.0, sun_altitude_degrees);
    float daylight = smootherstep_range(-2.0, 12.0, sun_altitude_degrees);

    float horizon = pow(1.0 - clamp(direction.y, 0.0, 1.0), 2.2);
    vec3 night_zenith = vec3(0.003, 0.006, 0.020);
    vec3 night_horizon = vec3(0.014, 0.020, 0.050);
    vec3 astronomical_zenith = vec3(0.012, 0.020, 0.060);
    vec3 astronomical_horizon = vec3(0.055, 0.055, 0.115);
    vec3 nautical_zenith = vec3(0.028, 0.060, 0.145);
    vec3 nautical_horizon = vec3(0.20, 0.12, 0.20);
    vec3 civil_zenith = vec3(0.075, 0.19, 0.43);
    vec3 civil_horizon = vec3(0.82, 0.31, 0.12);
    vec3 day_zenith = vec3(0.10, 0.34, 0.82);
    vec3 day_horizon = vec3(0.54, 0.72, 0.94);

    vec3 night_sky = mix(night_zenith, night_horizon, horizon);
    vec3 astronomical_sky = mix(astronomical_zenith, astronomical_horizon, horizon);
    vec3 nautical_sky = mix(nautical_zenith, nautical_horizon, horizon);
    vec3 civil_sky = mix(civil_zenith, civil_horizon, horizon);
    vec3 day_sky = mix(day_zenith, day_horizon, horizon);

    vec3 color = mix(night_sky, astronomical_sky, astronomical);
    color = mix(color, nautical_sky, nautical);
    color = mix(color, civil_sky, civil);
    color = mix(color, day_sky, daylight);

    float sun_forward = max(dot(direction, sun_direction), 0.0);
    float horizon_band = exp(-abs(direction.y) * 5.2);
    float warm_twilight =
        smootherstep_range(-14.0, -3.0, sun_altitude_degrees)
        * (1.0 - smootherstep_range(5.0, 22.0, sun_altitude_degrees));
    vec3 sunset_tint = vec3(1.0, 0.22, 0.045)
        * pow(sun_forward, 8.0)
        * horizon_band
        * warm_twilight;
    color += sunset_tint * 1.20;

    vec4 cloud_noise = texture(sampler2D(t_base_noise, s_sky), v_uv);
    float detail = texture(sampler2D(t_detail_noise, s_sky), v_uv * 2.0).r;
    float cloud = clamp(cloud_noise.r * 0.68 + detail * 0.32, 0.0, 1.0);
    vec3 cloud_day = mix(color * 0.78, vec3(0.92, 0.95, 1.0), cloud * 0.68);
    vec3 cloud_twilight = mix(color * 0.72, vec3(0.38, 0.24, 0.30), cloud * 0.30);
    vec3 cloud_night = mix(color, vec3(0.045, 0.055, 0.095), cloud * 0.20);
    vec3 cloud_transition = mix(cloud_night, cloud_twilight, nautical);
    color = mix(cloud_transition, cloud_day, daylight);

    vec3 stars = texture(sampler2D(t_starfield, s_sky), v_uv).rgb;
    float star_visibility =
        1.0 - smootherstep_range(-16.0, -2.0, sun_altitude_degrees);
    star_visibility = star_visibility * star_visibility
        * (3.0 - 2.0 * star_visibility);
    color += stars * 3.0 * vec3(0.72, 0.82, 1.0) * star_visibility;

    for (int i = 0; i < visual_count; ++i) {
        vec3 source_direction = normalize(sky.visual_dir_size[i].xyz);
        float angular_radius = radians(max(sky.visual_dir_size[i].w, 0.001) * 0.5);
        float halo_radius = radians(
            max(sky.visual_halo_kind[i].x, sky.visual_dir_size[i].w) * 0.5
        );
        float kind = sky.visual_halo_kind[i].z;
        float d = clamp(dot(direction, source_direction), -1.0, 1.0);

        float disc_inner = cos(angular_radius);
        float disc_outer = cos(angular_radius * 1.10);
        float disc = smoothstep(disc_outer, disc_inner, d);

        float halo_outer = cos(halo_radius);
        float halo = smoothstep(halo_outer, disc_outer, d)
            * max(sky.visual_halo_kind[i].y, 0.0);

        vec3 visual_color = max(sky.visual_color_intensity[i].rgb, vec3(0.0));
        float intensity = max(sky.visual_color_intensity[i].a, 0.0);

        if (kind < 0.5) {
            // Stellar disc: compact photosphere with mild limb darkening.
            float center = smoothstep(disc_outer, 1.0, d);
            float limb = mix(0.72, 1.0, sqrt(clamp(center, 0.0, 1.0)));
            color += visual_color * intensity * (disc * limb + halo);
        } else {
            // Billboard celestial body: use detail texture to modulate a lunar disc.
            vec3 tangent = make_tangent(source_direction);
            vec3 bitangent = normalize(cross(source_direction, tangent));
            float denom = max(tan(angular_radius), 0.00001);
            vec2 local = vec2(dot(direction, tangent), dot(direction, bitangent)) / denom;
            vec2 moon_uv = local * 0.5 + 0.5;
            vec4 billboard_sample = texture(
                sampler2D(t_billboard, s_sky),
                clamp(moon_uv, vec2(0.0), vec2(1.0))
            );
            float limb = sqrt(clamp(1.0 - dot(local, local), 0.0, 1.0));
            float texture_alpha = max(
                billboard_sample.a,
                dot(billboard_sample.rgb, vec3(0.299, 0.587, 0.114))
            );
            vec3 textured_surface = billboard_sample.rgb * visual_color;
            float textured_disc = disc * texture_alpha * mix(0.68, 1.0, limb);
            color += textured_surface * intensity * textured_disc;
            color += visual_color * intensity * halo * 0.35;
        }
    }

    // Screen-space optical flare derived from the script-driven stellar visual.
    // It is rendered inside the sky pass, so foreground scene geometry naturally
    // overwrites it later without requiring provider-specific blend support.
    vec2 pixel_ndc = project_direction(direction);
    for (int i = 0; i < visual_count; ++i) {
        float kind = sky.visual_halo_kind[i].z;
        if (kind >= 0.5) {
            continue;
        }

        vec3 source_direction = normalize(sky.visual_dir_size[i].xyz);
        float source_view_z = dot(normalize(sky.forward.xyz), source_direction);
        if (source_view_z <= 0.0001 || source_direction.y <= -0.04) {
            continue;
        }

        vec2 source_ndc = project_direction(source_direction);
        float source_edge = max(abs(source_ndc.x), abs(source_ndc.y));
        if (source_edge >= 1.08) {
            continue;
        }

        float source_intensity = max(sky.visual_color_intensity[i].a, 0.0);
        float visibility = (1.0 - smoothstep(0.68, 1.08, source_edge))
            * smootherstep_range(-0.14, 0.10, source_direction.y)
            * clamp(source_intensity / 9.5, 0.0, 1.0);
        vec3 flare_color = max(sky.visual_color_intensity[i].rgb, vec3(0.0));

        float source_glow = flare_blob(pixel_ndc, source_ndc, 0.20, 2.6);
        float source_core = flare_blob(pixel_ndc, source_ndc, 0.050, 6.0);
        float streak = exp(-abs(pixel_ndc.y - source_ndc.y) * 30.0)
            * exp(-abs(pixel_ndc.x - source_ndc.x) * 2.2)
            * 0.10;

        vec2 ghost_a = source_ndc * 0.46;
        vec2 ghost_b = source_ndc * -0.18;
        vec2 ghost_c = source_ndc * -0.66;
        float ghosts =
            flare_blob(pixel_ndc, ghost_a, 0.050, 4.2) * 0.16
            + flare_blob(pixel_ndc, ghost_b, 0.032, 5.2) * 0.11
            + flare_blob(pixel_ndc, ghost_c, 0.070, 3.8) * 0.075;

        vec3 ghost_tint = vec3(0.48, 0.63, 1.0);
        color += flare_color * visibility
            * (source_glow * 0.24 + source_core * 0.34 + streak);
        color += mix(flare_color, ghost_tint, 0.55) * visibility * ghosts;
    }

    // Gentle filmic shoulder keeps the sun emissive without flattening the sky.
    color = color / (vec3(1.0) + color * 0.16);
    out_color = vec4(max(color, vec3(0.0)), 1.0);
}
