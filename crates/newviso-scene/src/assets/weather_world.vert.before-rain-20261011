#version 450

layout(set = 0, binding = 0, std140) uniform WeatherWorld {
    mat4 view_proj;
    vec4 camera_time;          // xyz camera, w seconds
    vec4 camera_right_tan_x;   // xyz right, w tanHalfFovX
    vec4 camera_up_tan_y;      // xyz up, w tanHalfFovY
    vec4 camera_forward;       // xyz forward, w outdoor exposure
    vec4 emitter_center_type;  // xyz GTA-local centre, w type: 0 drop, 1 mist, 2 ground
    vec4 emitter_size_intensity; // xyz GTA-local half/full box extents, w intensity
    vec4 life_gravity_wind;    // x life min, y life max, z gravity, w wind influence
    vec4 velocity_min;
    vec4 velocity_max;
    vec4 sprite_sheet;         // x rows, y cols, z start frame, w end frame
    vec4 size_min_max;         // x min width, y max width, z min height, w max height
    vec4 color;
    vec4 fade_near_far;        // x near, y far, z fade in, w fade out
    vec4 wind_ground;          // xy wind dir, z wind speed, w ground level
} weather;

layout(location = 0) out vec2 v_uv;
layout(location = 1) out vec4 v_color;
layout(location = 2) out float v_life;
layout(location = 3) out float v_distance_fade;

float hash11(float p) {
    p = fract(p * 0.1031);
    p *= p + 33.33;
    p *= p + p;
    return fract(p);
}

float hash21(vec2 p) {
    vec3 p3 = fract(vec3(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

vec2 quad_corner(int index) {
    const vec2 corners[6] = vec2[6](
        vec2(-1.0, -1.0), vec2( 1.0, -1.0), vec2( 1.0,  1.0),
        vec2(-1.0, -1.0), vec2( 1.0,  1.0), vec2(-1.0,  1.0)
    );
    return corners[index];
}

vec2 quad_uv(int index) {
    const vec2 uv[6] = vec2[6](
        vec2(0.0, 1.0), vec2(1.0, 1.0), vec2(1.0, 0.0),
        vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(0.0, 0.0)
    );
    return uv[index];
}

void main() {
    int vertex_in_quad = gl_VertexIndex % 6;
    int particle_id = gl_VertexIndex / 6;
    float fid = float(particle_id);

    float life_min = max(weather.life_gravity_wind.x, 0.02);
    float life_max = max(weather.life_gravity_wind.y, life_min);
    float life = mix(life_min, life_max, hash11(fid * 3.17 + 0.73));
    float phase = fract(weather.camera_time.w / life + hash11(fid * 7.31 + 1.91));
    float age = phase * life;

    vec3 rnd = vec3(
        hash11(fid * 13.1 + 0.1),
        hash11(fid * 17.7 + 3.2),
        hash11(fid * 23.9 + 8.3)
    ) * 2.0 - 1.0;

    vec3 local = weather.emitter_center_type.xyz
        + rnd * weather.emitter_size_intensity.xyz * 0.5;

    vec3 velocity = mix(
        weather.velocity_min.xyz,
        weather.velocity_max.xyz,
        vec3(
            hash11(fid * 5.9 + 2.1),
            hash11(fid * 9.7 + 4.4),
            hash11(fid * 11.3 + 7.7)
        )
    );

    vec2 wind_dir = weather.wind_ground.xy;
    float wind_len = length(wind_dir);
    if (wind_len > 1e-5) {
        wind_dir /= wind_len;
    } else {
        wind_dir = vec2(1.0, 0.0);
    }
    float wind_speed = max(weather.wind_ground.z, 0.0) * weather.life_gravity_wind.w;

    local.x += velocity.x * age + wind_dir.x * wind_speed * age;
    local.y += velocity.y * age + wind_dir.y * wind_speed * age;
    local.z += velocity.z * age + 0.5 * weather.life_gravity_wind.z * age * age;

    vec3 right = normalize(weather.camera_right_tan_x.xyz);
    vec3 up = normalize(weather.camera_up_tan_y.xyz);
    vec3 forward = normalize(weather.camera_forward.xyz);

    // GTA weather emitter coordinates are interpreted as right / forward / up
    // around the active camera. NewViso remains Y-up in world space.
    vec3 centre = weather.camera_time.xyz
        + right * local.x
        + forward * local.y
        + up * local.z;

    float type = weather.emitter_center_type.w;
    float width = mix(
        weather.size_min_max.x,
        weather.size_min_max.y,
        hash11(fid * 31.1 + 0.8)
    );
    float height = mix(
        weather.size_min_max.z,
        weather.size_min_max.w,
        hash11(fid * 37.9 + 9.1)
    );

    vec2 corner = quad_corner(vertex_in_quad);
    vec3 quad_right = right;
    vec3 quad_up = up;
    if (type > 1.5) {
        // Ground systems are projected onto the world horizontal plane.
        centre.y = weather.wind_ground.w + 0.015;
        quad_right = vec3(1.0, 0.0, 0.0);
        quad_up = vec3(0.0, 0.0, 1.0);
    } else if (type > 0.5) {
        // Mist sheets are broad camera-facing volumes.
        width *= 0.5;
        height *= 0.5;
    }

    vec3 world = centre
        + quad_right * corner.x * max(width, 0.002) * 0.5
        + quad_up * corner.y * max(height, 0.002) * 0.5;

    gl_Position = weather.view_proj * vec4(world, 1.0);

    float rows = max(weather.sprite_sheet.x, 1.0);
    float cols = max(weather.sprite_sheet.y, 1.0);
    int first_frame = int(max(weather.sprite_sheet.z, 0.0) + 0.5);
    int last_frame = max(first_frame, int(weather.sprite_sheet.w + 0.5));
    int frame_count = max(last_frame - first_frame + 1, 1);
    int frame = first_frame + int(mod(
        floor(phase * float(frame_count)) + floor(hash11(fid * 43.1) * float(frame_count)),
        float(frame_count)
    ));
    int col = frame % int(cols);
    int row = frame / int(cols);
    v_uv = (vec2(float(col), float(row)) + quad_uv(vertex_in_quad)) / vec2(cols, rows);

    float distance_to_camera = length(world - weather.camera_time.xyz);
    float near_fade = smoothstep(
        max(weather.fade_near_far.x * 0.5, 0.0),
        max(weather.fade_near_far.x, 0.001),
        distance_to_camera
    );
    float far_fade = 1.0 - smoothstep(
        max(weather.fade_near_far.y * 0.75, weather.fade_near_far.x + 0.01),
        max(weather.fade_near_far.y, weather.fade_near_far.x + 0.02),
        distance_to_camera
    );

    v_color = weather.color;
    v_color.a *= clamp(weather.emitter_size_intensity.w, 0.0, 1.0)
        * clamp(weather.camera_forward.w, 0.0, 1.0);
    v_life = phase;
    v_distance_fade = near_fade * far_fade;
}
