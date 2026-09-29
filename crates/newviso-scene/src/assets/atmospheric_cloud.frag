#version 450

layout(set = 0, binding = 0, std140) uniform AtmosphericCloudFrame {
    mat4 view_proj;
    mat4 model;
    vec4 camera_alpha;
    vec4 sun_direction_intensity;
    vec4 layer_color_density;
    vec4 layer_params;
    vec4 uv_layer0;
    vec4 uv_layer1;
    vec4 uv_layer2;
    vec4 cloud_day_shadow;
    vec4 cloud_day_light;
    vec4 cloud_night;
    vec4 soft_depth_params;
    vec4 density_shift_scale;
    vec4 scatter;
    vec4 piercing;
    vec4 scale_diffuse_fill_ambient;
    vec4 wrap_lighting;
    vec4 anim_combine;
    vec4 anim_sculpt;
    vec4 anim_blend_weights;
    vec4 rescale_uv12;
    vec4 rescale_uv3_layer1;
    vec4 layer_anim_scale23;
    vec4 cloud_twilight_shadow;
    vec4 cloud_twilight_light;
    vec4 atmosphere_twilight;
    vec4 atmosphere_daylight_misc;
    vec4 silver_lining;
    vec4 environment_fog_color_density;
    vec4 environment_fog_params;
    vec4 environment_haze_color_density;
    vec4 environment_haze_params;
    vec4 key_cloud_color;
    vec4 key_cloud_light_color;
    vec4 key_cloud_ambient_color;
    vec4 key_cloud_sky_color;
    vec4 key_cloud_bounce_color;
    vec4 key_cloud_east_color;
    vec4 key_cloud_west_color;
    vec4 key_scale_fill_colors;
    vec4 key_density_shift_scale_scattering;
    vec4 key_piercing_light;
    vec4 key_scale_diffuse_fill_ambient_wrap;
    vec4 keyframe_meta;
} cloud;

layout(set = 0, binding = 1) uniform texture2D t_density;
layout(set = 0, binding = 2) uniform texture2D t_normal;
layout(set = 0, binding = 3) uniform texture2D t_detail_density;
layout(set = 0, binding = 4) uniform texture2D t_detail_normal;
layout(set = 0, binding = 5) uniform texture2D t_detail_density2;
layout(set = 0, binding = 6) uniform texture2D t_detail_normal2;
layout(set = 0, binding = 7) uniform sampler s_cloud;

layout(set = 1, binding = 0) uniform texture2D t_scene_linear_depth;
layout(set = 1, binding = 1) uniform sampler s_scene_depth;

layout(location = 0) in vec2 v_uv0;
layout(location = 1) in vec2 v_uv1;
layout(location = 2) in vec2 v_uv2;
layout(location = 3) in vec3 v_normal;
layout(location = 4) in vec3 v_tangent;
layout(location = 5) in vec3 v_binormal;
layout(location = 6) in vec4 v_color;
layout(location = 7) in vec3 v_world_position;
layout(location = 0) out vec4 out_color;

float smootherstep_range(float edge0, float edge1, float value) {
    float width = edge1 - edge0;
    if (abs(width) <= 1.0e-6) {
        return value >= edge1 ? 1.0 : 0.0;
    }
    float t = clamp((value - edge0) / width, 0.0, 1.0);
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

vec3 unpack_cloud_normal(vec2 encoded) {
    vec2 xy = encoded * 2.0 - 1.0;
    float z = sqrt(max(1.0 - dot(xy, xy), 0.0));
    return vec3(xy, z);
}

vec3 tangent_to_world(vec3 n) {
    return normalize(
        v_tangent * n.x
        + v_binormal * n.y
        + v_normal * n.z
    );
}

float animated_density(
    float d0,
    float d1,
    vec4 d2_sample
) {
    float d2 = d2_sample.g;

    // Exact scalar shape path from clouds_animsoft CloudsPS:
    // r3.xyz = (1 - density^2) * gAnimBlendWeights.
    vec3 shaped = (vec3(1.0) - vec3(d0, d1, d2) * vec3(d0, d1, d2))
        * cloud.anim_blend_weights.xyz;

    // gAnimSculpt is separate from the normal-combine weights. Vertex blue
    // seeds the sculpt field, while vertex green controls DetailDensity2.
    float sculpt = dot(shaped, cloud.anim_sculpt.xyz) + v_color.b;
    float detail_max = max(d2_sample.r, max(d2_sample.g, d2_sample.b));
    float detail_term = detail_max * v_color.g;
    float preserve = 1.0 - v_color.g * detail_max;
    float density = detail_term - preserve * sculpt;

    vec4 density_params = cloud.keyframe_meta.x > 0.5
        ? cloud.key_density_shift_scale_scattering
        : cloud.density_shift_scale;
    density = clamp(
        (density - density_params.x) * density_params.y,
        0.0,
        1.0
    );
    return density;
}

vec3 animated_normal(
    vec3 n0,
    vec3 n1,
    vec3 n2,
    float d0,
    float d1,
    float d2
) {
    vec3 shaped = (vec3(1.0) - vec3(d0, d1, d2) * vec3(d0, d1, d2))
        * cloud.anim_blend_weights.xyz;
    vec3 weights = shaped * cloud.anim_combine.xyz;
    vec3 combined = n0 * weights.x + n1 * weights.y + n2 * weights.z;
    float l2 = dot(combined, combined);
    if (l2 <= 1.0e-8) {
        return n0;
    }
    return combined * inversesqrt(l2);
}

void main() {
    float transition_alpha = clamp(cloud.camera_alpha.w, 0.0, 1.0);
    if (transition_alpha <= 0.0001) {
        discard;
    }

    vec4 density0_sample = texture(sampler2D(t_density, s_cloud), v_uv0);
    float d0 = density0_sample.g;
    vec3 n0 = unpack_cloud_normal(
        texture(sampler2D(t_normal, s_cloud), v_uv0).rg
    );

    // GTA declares gAnimCombine/gAnimSculpt/gAnimBlendWeights as float3.
    // combine.w is NewViso-only padding used to select the six-sampler
    // AnimSoft variant without contaminating any native XYZ component.
    bool animated = cloud.anim_combine.w > 0.5;
    float density;
    vec3 normal_ts;

    if (animated) {
        vec4 detail_density1_sample =
            texture(sampler2D(t_detail_density, s_cloud), v_uv1);
        vec4 detail_density2_sample =
            texture(sampler2D(t_detail_density2, s_cloud), v_uv2);
        float d1 = detail_density1_sample.g;
        float d2 = detail_density2_sample.g;

        vec3 n1 = unpack_cloud_normal(
            texture(sampler2D(t_detail_normal, s_cloud), v_uv1).rg
        );
        vec3 n2 = unpack_cloud_normal(
            texture(sampler2D(t_detail_normal2, s_cloud), v_uv2).rg
        );

        density = animated_density(d0, d1, detail_density2_sample);
        normal_ts = animated_normal(n0, n1, n2, d0, d1, d2);
    } else {
        // Non-AnimSoft CloudsPS variants use the green density channel
        // directly as inverse opacity.
        vec4 density_params = cloud.keyframe_meta.x > 0.5
            ? cloud.key_density_shift_scale_scattering
            : cloud.density_shift_scale;
        density = clamp(
            ((1.0 - d0) - density_params.x) * density_params.y,
            0.0,
            1.0
        );
        normal_ts = n0;
    }

    density = clamp(density * max(cloud.layer_color_density.w, 0.0), 0.0, 1.0);

    bool has_keyframe = cloud.keyframe_meta.x > 0.5;

    // No transition-progress input exists in the original AnimSoft pixel
    // shader. GTA CloudHat therefore keeps the source density closure intact
    // here and applies container transition alpha later. The density-threshold
    // morph remains only for generic NewViso atmospheric layers.
    if (!has_keyframe) {
        float shape_progress = clamp(cloud.layer_params.w, 0.0, 1.0);
        float shape_softness = max(cloud.layer_params.x, 1.0e-4);
        float shape_threshold = mix(
            1.0 + shape_softness,
            -shape_softness,
            shape_progress
        );
        float formation = smoothstep(
            shape_threshold - shape_softness,
            shape_threshold + shape_softness,
            density
        );
        density *= formation;
    }

    float native_alpha = density * clamp(v_color.a, 0.0, 1.0);

    vec4 frame_density = has_keyframe
        ? cloud.key_density_shift_scale_scattering
        : cloud.density_shift_scale;
    vec4 frame_piercing = has_keyframe
        ? cloud.key_piercing_light
        : cloud.piercing;
    vec4 frame_scale = has_keyframe
        ? cloud.key_scale_diffuse_fill_ambient_wrap
        : cloud.scale_diffuse_fill_ambient;
    vec3 frame_west = has_keyframe
        ? cloud.key_cloud_west_color.rgb
        : cloud.cloud_day_shadow.rgb;
    vec3 frame_east = has_keyframe
        ? cloud.key_cloud_east_color.rgb
        : cloud.cloud_day_light.rgb;
    vec3 frame_ambient = has_keyframe
        ? cloud.key_cloud_ambient_color.rgb
        : cloud.cloud_day_shadow.rgb;
    vec3 frame_sky = has_keyframe
        ? cloud.key_cloud_sky_color.rgb
        : cloud.cloud_day_shadow.rgb;
    vec3 frame_bounce = has_keyframe
        ? cloud.key_cloud_bounce_color.rgb
        : cloud.cloud_night.rgb;
    vec3 frame_light = has_keyframe
        ? cloud.key_cloud_light_color.rgb
        : cloud.cloud_day_light.rgb;
    vec3 frame_tint = has_keyframe
        ? cloud.key_cloud_color.rgb
        : vec3(1.0);
    // CloudScaleFillColors is a CPU-side CloudHat keyframe control rather
    // than a CloudsPS cbuffer field. Preserve it in runtime state, but do not
    // invent a shader-side global gain for it.
    vec3 N = tangent_to_world(normalize(normal_ts));
    vec3 L = normalize(cloud.sun_direction_intensity.xyz);
    vec3 V = normalize(cloud.camera_alpha.xyz - v_world_position);

    // Reconstruct the hemispheric term used by CloudsPS from the normal's
    // horizontal/vertical components, then feed it with NewViso's current
    // timecycle cloud colors. Exact GTA cloudkeyframe color curves can replace
    // these three runtime colors without changing this material ABI.
    vec3 hemi = clamp(
        vec3(
            N.x * 0.5 + 0.5,
            N.z * -0.8 + 0.2,
            N.z * 0.571429 + 0.428571
        ),
        0.0,
        1.0
    );
    vec3 west = frame_west;
    vec3 east_minus_west = frame_east - west;
    // Exact CloudsPS ordering from PSCloudsVertScatterPiercing_AnimSoft:
    // horizontal west/east, then Bounce on hemi.y and Sky on hemi.z.
    vec3 base_fill = west + east_minus_west * hemi.x;
    base_fill += frame_bounce * hemi.y;
    base_fill += frame_sky * hemi.z;

    float ndotl = dot(N, L);
    float wrapped = has_keyframe
        ? clamp(ndotl * frame_scale.w + (1.0 - frame_scale.w), 0.0, 1.0)
        : clamp(
            ndotl * cloud.wrap_lighting.x + cloud.wrap_lighting.y,
            0.0,
            1.0
        );
    vec3 sun_color = frame_light * max(cloud.sun_direction_intensity.w, 0.0);
    vec3 lit =
        sun_color * wrapped * frame_scale.x
        + base_fill * frame_scale.y
        + frame_ambient * frame_scale.z;

    // Exact phase-function inputs from CloudsVS. V points fragment->camera,
    // while the original shader uses camera->fragment for the sun cosine.
    float view_sun = clamp(dot(-V, L), -1.0, 1.0);
    float phase_denom = max(
        abs(1.0 + cloud.scatter.y - 2.0 * cloud.scatter.x * view_sun),
        1.0e-4
    );
    float phase = (1.0 + view_sun * view_sun)
        / pow(phase_denom, 1.5);
    float scattering_const = has_keyframe ? frame_density.z : cloud.scatter.z;
    float scattering_scale = has_keyframe ? frame_density.w : cloud.scatter.w;
    phase *= scattering_const * scattering_scale;
    vec3 scatter_light = sun_color * phase;

    float inverse_density = 1.0 - density;
    float thickness = max(frame_piercing.w, 0.0);
    float transmission = clamp(
        inverse_density * thickness + (1.0 - thickness),
        0.0,
        1.0
    );

    // CloudsPS applies CloudColor only to the diffuse/fill/ambient term.
    // Forward scattering is added independently.
    vec3 color = max(frame_tint, vec3(0.0)) * lit
        + scatter_light * transmission;

    // Exact piercing branch reconstructed from the original VS/PS sequence.
    // gPiercing.z scales the projected-normal term; it does not alter the
    // sampled normal map itself.
    float view_dot_sun = dot(V, L);
    vec3 piercing_dir_raw = V - L * view_dot_sun;
    float piercing_dir_l2 = dot(piercing_dir_raw, piercing_dir_raw);
    vec3 piercing_dir = piercing_dir_l2 > 1.0e-8
        ? piercing_dir_raw * inversesqrt(piercing_dir_l2)
        : vec3(0.0);
    float piercing_normal = clamp(dot(N, piercing_dir), 0.0, 1.0)
        * max(frame_piercing.z, 0.0);
    float piercing_view = pow(
        clamp(dot(-V, L), 0.0, 1.0),
        max(frame_piercing.x, 0.001)
    );
    float piercing_shape = piercing_view
        * (piercing_view + piercing_normal * (1.0 - piercing_view));
    color += sun_color
        * (piercing_shape * transmission * max(frame_piercing.y, 0.0));

    // Use the same solar-altitude bands as the sky renderer instead of the
    // previous one-step night/day approximation. This makes CloudHat color
    // track astronomical/nautical/civil twilight and daylight continuously.
    float sun_altitude_degrees = degrees(asin(clamp(L.y, -1.0, 1.0)));
    float nautical = smootherstep_range(
        cloud.atmosphere_twilight.y,
        cloud.atmosphere_twilight.z,
        sun_altitude_degrees
    );
    float daylight = smootherstep_range(
        cloud.atmosphere_daylight_misc.x,
        cloud.atmosphere_daylight_misc.y,
        sun_altitude_degrees
    );
    float warm_twilight =
        smootherstep_range(
            cloud.atmosphere_twilight.x,
            cloud.atmosphere_twilight.z,
            sun_altitude_degrees
        )
        * (1.0 - smootherstep_range(
            cloud.atmosphere_twilight.w,
            cloud.atmosphere_daylight_misc.y,
            sun_altitude_degrees
        ));

    float facing_sun = clamp(dot(N, L) * 0.5 + 0.5, 0.0, 1.0);
    float forward_scatter = pow(max(dot(-V, L), 0.0), 4.0);
    float twilight_light_mix = clamp(
        0.10 + facing_sun * 0.58 + forward_scatter * 0.32,
        0.0,
        1.0
    );
    vec3 twilight_color = mix(
        cloud.cloud_twilight_shadow.rgb,
        cloud.cloud_twilight_light.rgb,
        twilight_light_mix
    );

    if (!has_keyframe) {
        vec3 night_color = cloud.cloud_night.rgb
            * (0.35 + 0.20 * clamp(hemi.z, 0.0, 1.0));
        color = mix(night_color, twilight_color, nautical);
        color = mix(
            color,
            lit + scatter_light * transmission,
            daylight
        );
    }

    // Back-lit low-density edges receive the same authored silver-lining tint
    // used by the sky system. The edge term intentionally depends on density,
    // so the cloud interior does not become uniformly emissive.
    float cloud_edge = smoothstep(0.12, 0.82, 1.0 - density);
    float lining_visibility = mix(0.24, 1.0, warm_twilight);
    if (!has_keyframe) {
        color += max(cloud.silver_lining.rgb, vec3(0.0))
            * max(cloud.silver_lining.a, 0.0)
            * forward_scatter
            * cloud_edge
            * lining_visibility
            * max(cloud.sun_direction_intensity.w, 0.0);
    }

    color *= max(cloud.layer_color_density.rgb, vec3(0.0));

    float shoulder = max(cloud.cloud_night.a, 0.0);
    color = color / (vec3(1.0) + color * shoulder);

    float camera_distance = length(v_world_position - cloud.camera_alpha.xyz);

    // Match the opaque scene's distance haze and height fog so distant
    // CloudHat geometry belongs to the same atmosphere instead of floating in
    // front of the horizon.
    float haze_distance = max(
        camera_distance - max(cloud.environment_haze_params.x, 0.0),
        0.0
    );
    float haze = 1.0 - exp(
        -max(cloud.environment_haze_color_density.a, 0.0) * haze_distance
    );
    color = mix(
        color,
        max(cloud.environment_haze_color_density.rgb, vec3(0.0)),
        clamp(haze, 0.0, 1.0)
    );

    float fog_distance = max(
        camera_distance - max(cloud.environment_fog_params.x, 0.0),
        0.0
    );
    float height_above_base = max(
        v_world_position.y - cloud.environment_fog_params.z,
        0.0
    );
    float height_factor = exp(
        -height_above_base * max(cloud.environment_fog_params.y, 0.0)
    );
    float fog = 1.0 - exp(
        -max(cloud.environment_fog_color_density.a, 0.0)
        * fog_distance
        * height_factor
    );
    fog = min(
        clamp(fog, 0.0, 1.0),
        clamp(cloud.environment_fog_params.w, 0.0, 1.0)
    );
    color = mix(
        color,
        max(cloud.environment_fog_color_density.rgb, vec3(0.0)),
        fog
    );

    float soft_intersection = 1.0;
    if (cloud.soft_depth_params.w > 0.5) {
        vec2 viewport_extent = max(cloud.soft_depth_params.xy, vec2(1.0));
        vec2 screen_uv = clamp(
            gl_FragCoord.xy / viewport_extent,
            vec2(0.0),
            vec2(1.0)
        );
        float scene_depth = texture(
            sampler2D(t_scene_linear_depth, s_scene_depth),
            screen_uv
        ).r;
        // GTA clouds_soft/clouds_animsoft compare the sampled scene depth
        // against the cloud fragment in the camera-depth domain. Our
        // perspective matrix stores forward/view depth in clip.w.
        float cloud_depth = max(
            (cloud.view_proj * vec4(v_world_position, 1.0)).w,
            0.0
        );
        float fade_distance = max(cloud.soft_depth_params.z, 0.0001);
        // Original clouds_soft/clouds_animsoft uses saturate(delta / range),
        // not smoothstep. The proxy already stores linear camera/view depth.
        soft_intersection = clamp(
            (scene_depth - cloud_depth) / fade_distance,
            0.0,
            1.0
        );
    }

    float alpha = native_alpha * transition_alpha * soft_intersection;
    if (alpha <= 0.001) {
        discard;
    }

    out_color = vec4(max(color, vec3(0.0)), alpha);
}
