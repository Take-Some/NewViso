#version 450

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
} sky;

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec2 in_uv;
layout(location = 0) out vec2 v_uv;
layout(location = 1) out vec3 v_direction;
layout(location = 2) noperspective out vec2 v_view_plane;

void main() {
    // GTA keeps the dome camera-relative horizontally while its vertical
    // origin remains anchored to the world horizon/water level. In view-relative
    // coordinates the camera X/Z cancel, leaving only this vertical offset.
    vec3 unit_direction = normalize(in_position);
    float dome_scale = max(sky.dome_geometry.x, 100.0);
    float horizon_to_camera = sky.dome_geometry.y - sky.dome_geometry.z;
    vec3 dome_relative =
        unit_direction * dome_scale + vec3(0.0, horizon_to_camera, 0.0);
    vec3 dome_direction = normalize(dome_relative);
    float view_x = dot(sky.right.xyz, dome_direction);
    float view_y = dot(sky.up.xyz, dome_direction);
    float view_z = dot(sky.forward.xyz, dome_direction);

    gl_Position = vec4(
        view_x * sky.projection.x,
        -view_y * sky.projection.y,
        view_z,
        view_z
    );

    v_uv = in_uv;
    v_direction = dome_direction;

    // x/z and y/z are linear in screen space after perspective division.
    // noperspective interpolation reconstructs the exact per-pixel view ray
    // and removes cloud scale changes caused by sparse/uneven dome topology.
    float safe_view_z = max(view_z, 0.0001);
    v_view_plane = vec2(view_x, view_y) / safe_view_z;
}
