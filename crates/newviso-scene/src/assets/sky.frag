#version 450
#extension GL_GOOGLE_include_directive : require

layout(set = 0, binding = 0, std140) uniform SkyFrame {
    vec4 right;
    vec4 up;
    vec4 forward;
    vec4 projection;
    // x = visual count, y = sky time seconds,
    // z = cloud horizon fade, w = clouds enabled.
    vec4 visual_globals;
    vec4 visual_dir_size[4];
    vec4 visual_color_intensity[4];
    vec4 visual_halo_kind[4];
    // x = coverage, y = density, z = softness, w = base scale.
    vec4 cloud_params;
    // xy = wind speed, z = detail scale.
    vec4 cloud_motion;

    // Project-authored atmosphere profile.
    // twilight: astronomical start/end, nautical end, civil end.
    vec4 atmosphere_twilight;
    // x/y = daylight start/end, z = horizon power, w = tonemap shoulder.
    vec4 atmosphere_daylight_misc;
    vec4 atmosphere_night_zenith;
    vec4 atmosphere_night_horizon;
    vec4 atmosphere_astronomical_zenith;
    vec4 atmosphere_astronomical_horizon;
    vec4 atmosphere_nautical_zenith;
    vec4 atmosphere_nautical_horizon;
    vec4 atmosphere_civil_zenith;
    vec4 atmosphere_civil_horizon;
    vec4 atmosphere_day_zenith;
    vec4 atmosphere_day_horizon;
    // rgb = tint, a = strength.
    vec4 atmosphere_sunset;
    vec4 atmosphere_cloud_night;
    vec4 atmosphere_cloud_twilight_shadow;
    vec4 atmosphere_cloud_twilight_light;
    vec4 atmosphere_cloud_day_shadow;
    vec4 atmosphere_cloud_day_light;
    // rgb = tint, a = intensity.
    vec4 atmosphere_stars;
    // x/y = visibility altitude start/end, z = cloud occlusion.
    vec4 atmosphere_stars_misc;
    // rgb = tint, a = strength.
    vec4 atmosphere_silver_lining;
    // x/y = cloud alpha range.
    vec4 atmosphere_cloud_misc;

    // Generic cloud morphology, authored by project/weather scripts.
    // x = macro scale, y = macro strength,
    // z = detail strength, w = micro-detail strength.
    vec4 cloud_shape;
    // x = erosion strength, y = domain-warp strength,
    // z = shape contrast.
    vec4 cloud_sculpt;
    // xy = differential layer/shear velocity, zw = stable seed offset.
    vec4 cloud_shear_seed;

    // GTA-derived motion model, kept source-format neutral:
    // xy = integrated wind-driven base-noise phase,
    // z = continuous time-cycle in days, w = phase multiplier.
    vec4 cloud_phase;
    // x = small/filler phase speed, y = overall-detail speed,
    // z = edge-detail speed, w = large/base speed (telemetry parity).
    vec4 cloud_layer_speeds;
    // x = dome scale, y = fixed horizon level, z = camera world height.
    vec4 dome_geometry;
    vec4 key_east;
    vec4 key_west;
    vec4 key_transition;
    vec4 key_zenith;
    vec4 key_zenith_transition;
    vec4 key_zenith_params;
    // x = azimuth position, y = HDR scale, z = enabled, w = lunar phase.
    vec4 key_misc;
    vec4 key_mie;
    vec4 key_sun_color;
    vec4 camera_position;
    vec4 environment_reference[64];
    vec4 key_cloud_generation;
    vec4 key_cloud_density;
    vec4 key_cloud_lighting;
    vec4 key_cloud_detail;
    vec4 key_cloud_small;
    vec4 key_cloud_small_color;
    vec4 key_cloud_base;
    vec4 key_cloud_mid;
    vec4 key_cloud_shadow;
} sky;

#define ENV_LANE(i) sky.environment_reference[i]
#include "environment.glsl"

layout(set = 0, binding = 1) uniform texture2D t_base_noise;
layout(set = 0, binding = 2) uniform texture2D t_starfield;
layout(set = 0, binding = 3) uniform texture2D t_detail_noise;
layout(set = 0, binding = 4) uniform texture2D t_billboard;
layout(set = 0, binding = 5) uniform sampler s_sky;

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec3 v_direction;
layout(location = 2) noperspective in vec2 v_view_plane;
layout(location = 0) out vec4 out_color;

const float PI = 3.14159265358979323846;
const float TAU = 6.28318530717958647692;

vec3 make_tangent(vec3 n) {
    vec3 axis = abs(n.y) < 0.98 ? vec3(0.0, 1.0, 0.0) : vec3(1.0, 0.0, 0.0);
    return normalize(cross(axis, n));
}

vec2 spherical_uv(vec3 direction) {
    vec3 n = normalize(direction);
    float longitude = atan(n.z, n.x);
    float latitude = asin(clamp(n.y, -1.0, 1.0));
    return vec2(longitude / TAU + 0.5, latitude / PI + 0.5);
}

float smootherstep_range(float edge0, float edge1, float value) {
    float t = clamp((value - edge0) / max(edge1 - edge0, 0.00001), 0.0, 1.0);
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

// Procedural clouds are sampled on a horizontal atmospheric plane. The sky
// sphere is only a rasterization hull; it must never become the cloud domain.
vec2 cloud_layer_uv(vec3 direction, float scale, vec2 offset) {
    vec3 n = normalize(direction);
    // Rays below the atmospheric horizon never contribute cloud. Near the
    // horizon we clamp the denominator only to keep derivatives finite; alpha
    // is faded out separately before this region becomes visible.
    float elevation = max(n.y, 0.055);
    vec2 plane = n.xz / elevation;
    return plane * scale * 0.34 + offset;
}


vec3 reconstruct_world_view_ray() {
    // v_view_plane is screen-linear (noperspective).  The dome is therefore
    // only a rasterization hull; cloud sampling no longer inherits its UVs,
    // radius, pole topology, or triangle density.
    return normalize(
        sky.forward.xyz
        + sky.right.xyz * v_view_plane.x
        + sky.up.xyz * v_view_plane.y
    );
}

float detail_noise_3d(vec3 direction, float scale, vec2 offset) {
    vec2 uv = cloud_layer_uv(direction, scale, offset);
    float a = texture(sampler2D(t_detail_noise, s_sky), uv).r;
    float b = texture(
        sampler2D(t_detail_noise, s_sky),
        uv * vec2(1.071, 0.937) + vec2(0.173, -0.291)
    ).r;
    return mix(a, b, 0.32);
}

vec3 base_noise_rgb_3d(vec3 direction, float scale, vec2 offset) {
    vec2 uv = cloud_layer_uv(direction, scale, offset);
    vec3 a = texture(sampler2D(t_base_noise, s_sky), uv).rgb;
    vec3 b = texture(
        sampler2D(t_base_noise, s_sky),
        uv * vec2(1.013, 0.987) + vec2(0.193, -0.271)
    ).gbr;
    return mix(a, b, 0.20);
}


vec3 warp_cloud_direction(
    vec3 direction,
    float scale,
    vec2 offset,
    float strength
) {
    if (strength <= 0.0001) {
        return normalize(direction);
    }

    vec3 n = normalize(direction);
    vec3 tangent = make_tangent(n);
    vec3 bitangent = normalize(cross(n, tangent));

    float warp_x = detail_noise_3d(
        n,
        max(scale * 0.71, 0.1),
        offset + vec2(0.173, -0.291)
    ) - 0.5;
    float warp_y = detail_noise_3d(
        n,
        max(scale * 0.93, 0.1),
        offset * vec2(-0.81, 1.17) + vec2(-0.347, 0.529)
    ) - 0.5;

    // The user-facing warp is normalized to [0,1]; keep the angular
    // displacement bounded so large weather changes never fold the sky field.
    vec3 warped = n
        + tangent * warp_x * strength * 0.20
        + bitangent * warp_y * strength * 0.20;
    warped.y = n.y;
    return normalize(warped);
}

float phase_from_cycle(float speed, float cycle_days) {
    // GTA's sky uses 0.5 + speed * cycleTime / 11 and wraps to [0,1).
    return fract(0.5 + speed * cycle_days / 11.0);
}

vec2 directional_phase_offset(vec2 wind_direction, float phase, float cross_bias) {
    vec2 perpendicular = vec2(-wind_direction.y, wind_direction.x);
    return wind_direction * phase + perpendicular * phase * cross_bias;
}

float cloud_mask(vec3 direction) {
    if (sky.visual_globals.w < 0.5) {
        return 0.0;
    }

    float coverage = clamp(sky.cloud_params.x, 0.0, 1.0);
    float density = clamp(sky.cloud_params.y, 0.0, 2.0);
    float softness = clamp(sky.cloud_params.z, 0.01, 1.0);
    float scale = max(sky.cloud_params.w, 0.05);
    float detail_scale = max(sky.cloud_motion.z, 0.1);

    float macro_scale = max(sky.cloud_shape.x, 0.02);
    float macro_strength = clamp(sky.cloud_shape.y, 0.0, 2.0);
    float detail_strength = clamp(sky.cloud_shape.z, 0.0, 2.0);
    float edge_strength = clamp(sky.cloud_shape.w, 0.0, 2.0);
    float erosion_strength = clamp(sky.cloud_sculpt.x, 0.0, 2.0);
    float warp_strength = clamp(sky.cloud_sculpt.y, 0.0, 1.0);
    float shape_contrast = clamp(sky.cloud_sculpt.z, 0.25, 4.0);

    // The project field historically named speed is now the generic global
    // air-wind vector. GTA normalizes global AIR velocity before integrating
    // the large-cloud phase, so magnitude is deliberately ignored here too.
    vec2 wind_vector = sky.cloud_motion.xy;
    float wind_length = length(wind_vector);
    vec2 wind_direction = wind_length > 0.000001
        ? wind_vector / wind_length
        : vec2(1.0, 0.0);

    vec2 seed = sky.cloud_shear_seed.zw;
    vec2 base_phase = sky.cloud_phase.xy;

    // GTA keeps three additional independently wrapped phases for filler
    // clouds, the overall detail overlay and edge detail.
    float cycle_days = sky.cloud_phase.z;
    float small_phase = phase_from_cycle(sky.cloud_layer_speeds.x, cycle_days);
    float overall_phase = phase_from_cycle(sky.cloud_layer_speeds.y, cycle_days);
    float edge_phase = phase_from_cycle(sky.cloud_layer_speeds.z, cycle_days);

    vec2 small_offset =
        base_phase * 0.37
        + directional_phase_offset(wind_direction, small_phase, 0.17)
        + seed * vec2(1.37, -0.81)
        + vec2(0.213, -0.377);
    vec2 overall_offset =
        base_phase * 0.61
        + directional_phase_offset(wind_direction, overall_phase, -0.11)
        + seed * vec2(-0.63, 1.51)
        + vec2(-0.491, 0.281);
    vec2 edge_offset =
        base_phase * 0.83
        + directional_phase_offset(wind_direction, edge_phase, 0.23)
        + seed * vec2(0.73, 1.19)
        + vec2(0.347, -0.159);

    // Preserve the optional generic shear channel for non-RSC7 weather
    // profiles, but keep it secondary to the explicit independent phases.
    float time_seconds = sky.visual_globals.y;
    vec2 shear = sky.cloud_shear_seed.xy * time_seconds;
    overall_offset += shear * 0.11;
    edge_offset += shear * 0.19;

    vec3 warped_direction = warp_cloud_direction(
        direction,
        scale * macro_scale,
        base_phase * 0.19 + seed * 0.071,
        warp_strength
    );

    // Large/base clouds: the authored three-channel Perlin source is moved by
    // the CPU-integrated wind phase, matching GTA's per-frame noisePhase path.
    vec3 large_rgb = base_noise_rgb_3d(
        warped_direction,
        scale,
        base_phase + seed
    );
    float large_max = max(large_rgb.r, max(large_rgb.g, large_rgb.b));
    float large_mean = dot(large_rgb, vec3(0.50, 0.31, 0.19));
    float large_body = mix(large_mean, large_max, 0.68);

    // Small/filler clouds use a separate time-cycle phase instead of sharing
    // the large-cloud UV transform.
    vec3 small_rgb = base_noise_rgb_3d(
        warped_direction,
        scale * 1.47,
        small_offset
    );
    float small_max = max(small_rgb.r, max(small_rgb.g, small_rgb.b));
    float small_mean = dot(small_rgb, vec3(0.21, 0.47, 0.32));
    float small_body = mix(small_mean, small_max, 0.54);

    // A slow macro field breaks repetition at weather scale. This is a
    // NewViso generic extension layered under the GTA phase model.
    vec3 macro_rgb = base_noise_rgb_3d(
        direction,
        scale * macro_scale,
        base_phase * 0.16 + seed * 0.29 + vec2(0.137, -0.223)
    );
    float macro =
        dot(macro_rgb, vec3(0.44, 0.35, 0.21)) * 0.62
        + max(macro_rgb.r, max(macro_rgb.g, macro_rgb.b)) * 0.38;

    float combined = large_body * 0.78 + small_body * 0.22;
    combined += (macro - 0.5) * macro_strength;

    // Overall detail and edge detail move on distinct phases, matching GTA's
    // speedConstants channels instead of advecting every octave together.
    float overall_detail = detail_noise_3d(
        warped_direction,
        scale * detail_scale,
        overall_offset
    );
    float pre_edge_shape =
        combined - (overall_detail - 0.5) * detail_strength * erosion_strength;

    float edge_detail = detail_noise_3d(
        warped_direction,
        scale * detail_scale * 2.0,
        edge_offset
    );
    float edge_band = 1.0 - smoothstep(
        0.12,
        0.42,
        abs(pre_edge_shape - mix(0.70, 0.38, coverage))
    );
    float shape =
        pre_edge_shape
        - (edge_detail - 0.5) * edge_strength * erosion_strength * edge_band;
    shape = (shape - 0.5) * shape_contrast + 0.5;

    float threshold = mix(0.84, 0.24, coverage);
    float mask = smoothstep(
        threshold - softness * 0.50,
        threshold + softness * 0.50,
        shape
    );
    mask = clamp(mask * density, 0.0, 1.0);

    // Never show procedural clouds on the lower hemisphere. The old
    // triplanar fallback made them visibly wrap underneath the observer.
    float horizon_fade = max(sky.visual_globals.z, 0.10);
    float horizon_visibility = smoothstep(0.045, horizon_fade, direction.y);
    return mask * horizon_visibility;
}

// Procedural skydome clouds follow the original three-channel Perlin pass.
// The pass is evaluated here from the preserved source DDS, without another
// render target; no synthetic coverage threshold replaces the TC parameters.
vec3 reference_perlin(vec2 uv) {
    vec4 generation = sky.key_cloud_generation;
    vec2 p = ((uv - 0.5) * generation.y + 0.5) / 128.0 + sky.cloud_phase.xy;
    float value = (texture(sampler2D(t_base_noise, s_sky), p).r - 0.5) * 2.0;
    p *= generation.x;
    value += (texture(sampler2D(t_base_noise, s_sky), p).g - 0.5);
    p *= generation.x;
    value += (texture(sampler2D(t_base_noise, s_sky), p).b - 0.5) * 0.5;
    float noise = value / 1.75 * 0.5 + 0.5;
    float softness = max(generation.w, 1.0e-5);
    vec2 bounds = vec2(generation.z - softness, generation.z + softness);
    return vec3(
        smoothstep(bounds.x, bounds.y, noise),
        smoothstep(bounds.x + sky.key_cloud_small_color.w,
            bounds.y + sky.key_cloud_small_color.w, noise),
        noise
    );
}

void reference_clouds(vec3 direction, vec3 sun_direction,
    out vec3 large_color, out vec2 alpha) {
    // The packaged high dome exposes an angular UV with a pole singularity.
    // Reconstruct a continuous hemisphere disk from the view ray instead.
    // The original solar warp and independent detail phases operate on this disk.
    float radius = max(sky.dome_geometry.x, 100.0);
    float center_y = sky.dome_geometry.y - sky.camera_position.y;
    float along_ray = direction.y * center_y;
    float distance_to_dome = max(along_ray + sqrt(max(radius * radius
        - center_y * center_y + along_ray * along_ray, 0.0)), 1.0);
    vec3 dome_direction = normalize(direction * distance_to_dome
        - vec3(0.0, center_y, 0.0));
    vec2 uv = vec2(dome_direction.x, -dome_direction.z)
        * (0.5 / (1.0 + max(dome_direction.y, 0.0))) + 0.5;
    vec2 solar_offset = sky.key_cloud_lighting.y / 10.0
        * vec2(sun_direction.x - direction.x, -sun_direction.z + direction.z);
    vec2 centered = uv * 2.0 - 1.0;
    vec2 shifted_uv = uv - solar_offset * (1.0 - dot(centered, centered));
    vec3 perlin = reference_perlin(uv);
    vec3 shifted = reference_perlin(shifted_uv);

    float small_phase = phase_from_cycle(sky.cloud_layer_speeds.x, sky.cloud_phase.z);
    float overall_phase = phase_from_cycle(sky.cloud_layer_speeds.y, sky.cloud_phase.z);
    float edge_phase = phase_from_cycle(sky.cloud_layer_speeds.z, sky.cloud_phase.z);
    float overall = texture(sampler2D(t_detail_noise, s_sky),
        (uv.yx - 0.5 + overall_phase) * sky.key_cloud_detail.w).r - 0.5;
    float edge = texture(sampler2D(t_detail_noise, s_sky),
        (uv - 0.5 + edge_phase) * sky.key_cloud_detail.y).r - 0.5;
    float small_edge = texture(sampler2D(t_detail_noise, s_sky),
        (uv - 0.5 + small_phase) * sky.key_cloud_small.x).r - 0.5;

    float base = (shifted.x + overall * sky.key_cloud_lighting.z
        * sky.key_cloud_detail.z) * sky.key_cloud_density.x * perlin.x;
    vec2 detail = vec2(overall, 1.0 - perlin.z)
        * vec2(sky.key_cloud_detail.z, sky.key_cloud_small.y);
    detail += vec2(edge, small_edge)
        * vec2(sky.key_cloud_detail.x, sky.key_cloud_small.y)
        * clamp(1.0 - base * 2.0, 0.0, 1.0) * perlin.z;
    vec2 density = clamp(
        vec2(sky.key_cloud_density.y, sky.key_cloud_small.z)
            * (vec2(perlin.x, 0.0) + detail)
        - vec2(sky.key_cloud_density.z, sky.key_cloud_small.w),
        0.0, 1.0
    );
    alpha = density * density
        * clamp(direction.y * 5.0 - sky.key_cloud_density.w, 0.0, 1.0);
    vec3 shadow_minus_base = (sky.key_cloud_shadow.rgb - sky.key_cloud_base.rgb)
        * sky.key_cloud_lighting.x;
    large_color = (base * ((shifted.y + detail.x) * shadow_minus_base
        + sky.key_cloud_base.rgb - sky.key_cloud_mid.rgb) + sky.key_cloud_mid.rgb)
        * sky.key_cloud_lighting.w;
}

void main() {
    vec3 direction = reconstruct_world_view_ray();
    int visual_count = clamp(int(sky.visual_globals.x + 0.5), 0, 4);

    vec3 sun_direction = vec3(0.0, -1.0, 0.0);
    float sun_intensity = 0.0;
    for (int i = 0; i < visual_count; ++i) {
        float drives_atmosphere = sky.visual_halo_kind[i].w;
        float intensity = max(sky.visual_color_intensity[i].a, 0.0);
        if (drives_atmosphere > 0.5 && intensity >= sun_intensity) {
            sun_intensity = intensity;
            sun_direction = normalize(sky.visual_dir_size[i].xyz);
        }
    }

    float sun_elevation = clamp(sun_direction.y, -1.0, 1.0);
    float sun_altitude_degrees = degrees(asin(sun_elevation));

    float astronomical = smootherstep_range(
        sky.atmosphere_twilight.x,
        sky.atmosphere_twilight.y,
        sun_altitude_degrees
    );
    float nautical = smootherstep_range(
        sky.atmosphere_twilight.y,
        sky.atmosphere_twilight.z,
        sun_altitude_degrees
    );
    float civil = smootherstep_range(
        sky.atmosphere_twilight.z,
        sky.atmosphere_twilight.w,
        sun_altitude_degrees
    );
    float daylight = smootherstep_range(
        sky.atmosphere_daylight_misc.x,
        sky.atmosphere_daylight_misc.y,
        sun_altitude_degrees
    );

    float sky_height = clamp(direction.y, 0.0, 1.0);
    float horizon = pow(
        1.0 - sky_height,
        max(sky.atmosphere_daylight_misc.z, 0.01)
    );

    vec3 night_sky = mix(
        sky.atmosphere_night_zenith.rgb,
        sky.atmosphere_night_horizon.rgb,
        horizon
    );
    vec3 astronomical_sky = mix(
        sky.atmosphere_astronomical_zenith.rgb,
        sky.atmosphere_astronomical_horizon.rgb,
        horizon
    );
    vec3 nautical_sky = mix(
        sky.atmosphere_nautical_zenith.rgb,
        sky.atmosphere_nautical_horizon.rgb,
        horizon
    );
    vec3 civil_sky = mix(
        sky.atmosphere_civil_zenith.rgb,
        sky.atmosphere_civil_horizon.rgb,
        horizon
    );
    vec3 day_sky = mix(
        sky.atmosphere_day_zenith.rgb,
        sky.atmosphere_day_horizon.rgb,
        horizon
    );

    vec3 color = mix(night_sky, astronomical_sky, astronomical);
    color = mix(color, nautical_sky, nautical);
    color = mix(color, civil_sky, civil);
    color = mix(color, day_sky, daylight);

    float sun_forward = max(dot(direction, sun_direction), 0.0);
    float horizon_band = exp(-abs(direction.y) * 5.2);
    float warm_twilight =
        smootherstep_range(
            sky.atmosphere_twilight.x,
            sky.atmosphere_twilight.z,
            sun_altitude_degrees
        )
        * (1.0 - smootherstep_range(
            sky.atmosphere_twilight.w,
            sky.atmosphere_daylight_misc.y,
            sun_altitude_degrees
        ));
    vec3 sunset_tint = sky.atmosphere_sunset.rgb
        * pow(sun_forward, 8.0)
        * horizon_band
        * warm_twilight;
    color += sunset_tint * sky.atmosphere_sunset.a;

    if (sky.key_misc.z > 0.5) {
        float azimuth = sqrt(clamp(-direction.x * 0.5 + 0.5, 0.0, 1.0));
        float az_pos = clamp(sky.key_misc.x, 0.0001, 0.9999);
        vec3 az_color = azimuth < az_pos
            ? mix(sky.key_east.rgb, sky.key_transition.rgb, azimuth / az_pos)
            : mix(sky.key_transition.rgb, sky.key_west.rgb, (azimuth - az_pos) / (1.0 - az_pos));
        float z_pos = clamp(sky.key_zenith_params.x, 0.0001, 0.9999);
        float z_blend = abs(direction.y);
        vec3 z_transition = mix(az_color, sky.key_zenith_transition.rgb,
            mix(sky.key_zenith_params.y, sky.key_zenith_params.z, azimuth));
        color = (z_blend < z_pos ? mix(az_color, z_transition, z_blend / z_pos)
            : mix(z_transition, sky.key_zenith.rgb, clamp((z_blend-z_pos)/(1.0-z_pos)
                / max(sky.key_zenith_params.w, 0.0001), 0.0, 1.0))) * max(sky.key_misc.y, 0.0);
        float cos_theta = dot(direction, sun_direction);
        float mie = sky.key_mie.z * (1.0 + cos_theta * cos_theta)
            / pow(max(sky.key_mie.y + sky.key_mie.x * cos_theta, 0.0001), 1.5);
        color += sky.key_sun_color.rgb * clamp(mie, 0.0, 1.0) * sky.key_mie.w;
    }

    float cloud = 0.0;
    if (sky.key_misc.z > 0.5 && sky.dome_geometry.w > 0.5) {
        if (sky.visual_globals.w > 0.5) {
            vec3 large_color;
            vec2 amount;
            reference_clouds(direction, sun_direction, large_color, amount);
            color = mix(color, sky.key_cloud_small_color.rgb, amount.y);
            color = mix(color, large_color, amount.x);
            cloud = amount.x + amount.y * (1.0 - amount.x);
        }
    } else {
    // Generic morphology remains available for independently authored skies.
    cloud = cloud_mask(direction);
    if (cloud > 0.0001) {
        float cycle_days = sky.cloud_phase.z;
        vec2 wind_vector = sky.cloud_motion.xy;
        float wind_length = length(wind_vector);
        vec2 wind_direction = wind_length > 0.000001
            ? wind_vector / wind_length
            : vec2(1.0, 0.0);
        float lighting_phase = phase_from_cycle(
            sky.cloud_layer_speeds.y,
            cycle_days
        );
        vec2 lighting_offset =
            sky.cloud_phase.xy * 0.73
            + directional_phase_offset(wind_direction, lighting_phase, -0.11)
            + sky.cloud_shear_seed.zw * 0.37;
        float cloud_detail = detail_noise_3d(
            direction,
            max(sky.cloud_params.w * sky.cloud_motion.z * 0.79, 0.1),
            lighting_offset
        );

        float sun_alignment = max(dot(direction, sun_direction), 0.0);
        float facing_sun = clamp(sun_alignment * 0.5 + 0.5, 0.0, 1.0);

        // Approximate optical thickness from the final coverage field.  Dense
        // interiors stay shaded while translucent edges receive forward
        // scattering.  This avoids the old flat uniformly-lit cloud sheet.
        float interior = smoothstep(0.32, 0.94, cloud);
        float edge = clamp(
            cloud * (1.0 - smoothstep(0.42, 0.96, cloud)) * 2.35,
            0.0,
            1.0
        );
        float forward_scatter = pow(sun_alignment, 6.0)
            * mix(0.28, 1.0, edge);
        float silver_lining = pow(sun_alignment, 10.0)
            * edge
            * (1.0 - interior * 0.35);

        float day_light_mix = clamp(
            0.29
            + facing_sun * 0.39
            + cloud_detail * 0.18
            + forward_scatter * 0.22
            - interior * 0.20,
            0.0,
            1.0
        );
        vec3 day_cloud = mix(
            sky.atmosphere_cloud_day_shadow.rgb,
            sky.atmosphere_cloud_day_light.rgb,
            day_light_mix
        );
        day_cloud = mix(
            day_cloud,
            sky.atmosphere_cloud_day_shadow.rgb,
            interior * clamp(sky.cloud_params.y - 0.55, 0.0, 1.0) * 0.22
        );

        float twilight_light_mix = clamp(
            facing_sun * 0.51
            + forward_scatter * 0.30
            + silver_lining * 0.34
            - interior * 0.16,
            0.0,
            1.0
        );
        vec3 twilight_cloud = mix(
            sky.atmosphere_cloud_twilight_shadow.rgb,
            sky.atmosphere_cloud_twilight_light.rgb,
            twilight_light_mix
        );

        vec3 cloud_color = mix(
            sky.atmosphere_cloud_night.rgb,
            twilight_cloud,
            nautical
        );
        cloud_color = mix(cloud_color, day_cloud, daylight);

        // Edge-forward scattering is useful in daylight too; sunset merely
        // warms it more strongly.
        float lining_visibility = mix(0.24, 1.0, warm_twilight);
        cloud_color += sky.atmosphere_silver_lining.rgb
            * silver_lining
            * lining_visibility
            * sky.atmosphere_silver_lining.a;
        cloud_color += sky.atmosphere_cloud_day_light.rgb
            * forward_scatter
            * daylight
            * edge
            * 0.09;

        float cloud_alpha = cloud * mix(
            sky.atmosphere_cloud_misc.x,
            sky.atmosphere_cloud_misc.y,
            clamp(sky.cloud_params.y, 0.0, 1.0)
        );
        color = mix(color, cloud_color, cloud_alpha);
    }
    }

    // Star field also uses world direction instead of imported skydome UVs.
    vec2 star_uv = spherical_uv(direction);
    vec3 stars = texture(sampler2D(t_starfield, s_sky), star_uv).rgb;
    float star_visibility =
        1.0 - smootherstep_range(
            sky.atmosphere_stars_misc.x,
            sky.atmosphere_stars_misc.y,
            sun_altitude_degrees
        );
    star_visibility = star_visibility * star_visibility
        * (3.0 - 2.0 * star_visibility);
    color += stars
        * sky.atmosphere_stars.a
        * sky.atmosphere_stars.rgb
        * star_visibility
        * (1.0 - cloud * sky.atmosphere_stars_misc.z);

    float local_cloud_visibility = 1.0 - cloud * sky.atmosphere_stars_misc.z;

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
            float center = smoothstep(disc_outer, 1.0, d);
            float limb = mix(0.72, 1.0, sqrt(clamp(center, 0.0, 1.0)));
            color += visual_color
                * intensity
                * (disc * limb + halo)
                * local_cloud_visibility;
        } else {
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
            float phase_light = 1.0;
            if (sky.key_misc.z > 0.5) {
                float phase_angle = TAU * sky.key_misc.w;
                // Full moon at phase 0.5, matching the original 27-day phase.
                vec3 moon_light = vec3(sin(phase_angle), 0.0, -cos(phase_angle));
                phase_light = smoothstep(-0.02, 0.05, dot(vec3(local, limb), moon_light));
            }
            float textured_disc = disc * texture_alpha * mix(0.68, 1.0, limb) * phase_light;
            color += textured_surface
                * intensity
                * textured_disc
                * local_cloud_visibility;
            color += visual_color
                * intensity
                * halo
                * 0.35
                * local_cloud_visibility;
        }
    }

    if (environment_enabled()) {
        // Integrate no-haze fog along the same reconstructed ray as the sky;
        // sparse dome triangles must not change atmospheric extinction.
        float radius = max(sky.dome_geometry.x, 100.0);
        float center_y = sky.dome_geometry.y - sky.camera_position.y;
        float along_ray = direction.y * center_y;
        float dome_distance = max(along_ray + sqrt(max(
            radius * radius - center_y * center_y + along_ray * along_ray, 0.0)), 1.0);
        vec4 fog=environment_fog(sky.camera_position.xyz,
            sky.camera_position.xyz+direction*dome_distance,0.0);
        color=mix(color,fog.rgb,fog.a);
        out_color=vec4(max(color,vec3(0.0)),1.0);
        return;
    }
    // Mild filmic shoulder keeps the sun emissive without flattening cloud
    // contrast or turning the whole sky white.
    color = color / (
        vec3(1.0) + color * max(sky.atmosphere_daylight_misc.w, 0.0)
    );
    out_color = vec4(max(color, vec3(0.0)), 1.0);
}

