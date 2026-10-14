#version 450

layout(set = 0, binding = 0, std140) uniform WeatherWorld {
    mat4 view_proj;
    vec4 camera_time;          // xyz camera, w seconds
    vec4 camera_right_tan_x;   // xyz right, w tanHalfFovX
    vec4 camera_up_tan_y;      // xyz up, w tanHalfFovY
    vec4 camera_forward;       // xyz forward, w outdoor exposure
    vec4 emitter_center_type;  // xyz absolute world centre, w type: 0 drop, 1 mist, 2 ground
    vec4 emitter_size_intensity; // xyz Y-up world box dimensions, w intensity
    vec4 life_gravity_wind;    // x life min, y life max, z gravity, w wind influence
    vec4 velocity_min;
    vec4 velocity_max;         // xyz source velocity, w stable horizontal seed phase X
    vec4 sprite_sheet;         // x rows, y cols, z start frame, w end frame
    vec4 size_min_max;         // x min width, y max width, z min height, w max height
    vec4 color;
    vec4 fade_near_far;        // x near, y far, z fade in, w fade out
    vec4 wind_ground;          // xz wind direction, speed, fallback ground
    vec4 animation;            // rate, over-life switch, second sheet start/end
    vec4 directional;          // directional switch, z-offset min/max, ground clamp
    vec4 camera_motion;        // relative streak velocity
    vec4 rotation;             // xy rotation speeds, z stable seed phase Z, w splash pair
    vec4 ground_fade;          // ground-relative lower/upper fade offsets
    vec4 collision_grid;       // world xz origin, cell metres, ready
    vec4 collision_heights[273]; // packed 33x33 surface heights + padding
} weather;

layout(location = 0) out vec2 v_uv;
layout(location = 1) out vec4 v_color;
layout(location = 2) out float v_life;
layout(location = 3) out float v_distance_fade;
layout(location = 4) out float v_alpha_fade;
layout(location = 5) out vec4 v_uv_bounds;
layout(location = 6) out float v_snow;

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


// A height field is a precipitation collision surface, independent of view pitch.
float surface_height(vec2 world_xz) {
    if (weather.collision_grid.w < 0.5) return -1000000.0;
    vec2 cell = (world_xz - weather.collision_grid.xy) / weather.collision_grid.z;
    if (any(lessThan(cell, vec2(0.0))) || any(greaterThan(cell, vec2(32.0))))
        return -1000000.0;
    ivec2 ij = ivec2(clamp(round(cell), vec2(0.0), vec2(32.0)));
    int index = ij.y * 33 + ij.x;
    return weather.collision_heights[index / 4][index % 4];
}
vec3 safe_normal(vec3 value, vec3 fallback) {
    return dot(value, value) > 1e-8 ? normalize(value) : fallback;
}
// Continuous solution to v' = gravity + windInfluence * (airVelocity - v).
// This preserves wind response without a CPU rigid body per droplet.
void trajectory(vec3 initial_velocity, float age, out vec3 displacement, out vec3 velocity) {
    float drag = max(weather.life_gravity_wind.w, 0.0);
    vec3 gravity = vec3(0.0, weather.life_gravity_wind.z, 0.0);
    vec2 wind = weather.wind_ground.xy;
    wind = length(wind) > 1e-5 ? normalize(wind) : vec2(1.0, 0.0);
    vec3 air = vec3(wind.x, 0.0, wind.y) * max(weather.wind_ground.z, 0.0);
    if (drag > 0.0001) {
        vec3 terminal = air + gravity / drag;
        float decay = exp(-drag * age);
        displacement = terminal * age + (initial_velocity - terminal) * (1.0 - decay) / drag;
        velocity = terminal + (initial_velocity - terminal) * decay;
    } else {
        displacement = initial_velocity * age + gravity * (0.5 * age * age);
        velocity = initial_velocity + gravity * age;
    }
}
void main() {
    float type = weather.emitter_center_type.w;
    bool ground = type > 1.5 && type < 2.5;
    bool splash_pair = ground && weather.rotation.w > 0.5;
    bool blowing_snow = ground && weather.velocity_min.w > 0.5;
    int vertices_per_particle = splash_pair ? 12 : 6;
    int particle_id = gl_VertexIndex / vertices_per_particle;
    int vertex_in_quad = gl_VertexIndex % 6;
    bool ripple = splash_pair && (gl_VertexIndex % 12) >= 6;
    float fid = float(particle_id);
    float life_min = max(weather.life_gravity_wind.x, 0.02);
    float life_max = max(weather.life_gravity_wind.y, life_min);
    float life = mix(life_min, life_max, hash11(fid * 3.17 + 0.73));
    float phase = fract(weather.camera_time.w / life + hash11(fid * 7.31 + 1.91));
    float age = phase * life;
    vec3 rnd = vec3(hash11(fid * 13.1 + 0.1), hash11(fid * 17.7 + 3.2),
                    hash11(fid * 23.9 + 8.3)) - 0.5;
    vec3 camera_right = safe_normal(weather.camera_right_tan_x.xyz, vec3(1,0,0));
    vec3 camera_forward = safe_normal(weather.camera_forward.xyz, vec3(0,0,1));
    // The view basis projects sprites only. The physical weather volume is
    // world-aligned, so yaw/pitch/roll never orbit or reseed mist and snow.
    vec3 box_centre = weather.emitter_center_type.xyz;
    vec3 size = max(weather.emitter_size_intensity.xyz, vec3(0.01));

    // Anchor seeds in world space. Camera motion changes the volume bounds,
    // not each particle's velocity or its vertical axis.
    // Imported view-ahead offsets shift the world seed lattice, never the
    // window with the view direction. Keep precipitation around all headings.
    vec3 seed_phase = vec3(weather.velocity_max.w,0.0,weather.rotation.z);
    vec3 anchor = floor((box_centre-seed_phase)/size)*size+seed_phase;
    vec3 centre = anchor + vec3(rnd.x,rnd.z,rnd.y) * size;
    vec3 v0 = mix(weather.velocity_min.xyz, weather.velocity_max.xyz,
        vec3(hash11(fid*5.9+2.1),hash11(fid*9.7+4.4),hash11(fid*11.3+7.7)));
    vec3 displacement, velocity;
    trajectory(vec3(v0.x,v0.z,v0.y), age, displacement, velocity);
    centre += displacement;
    centre.xz -= size.xz * floor((centre.xz-box_centre.xz)/size.xz+0.5);
    if (!ground)
        centre.y -= size.y * floor((centre.y-box_centre.y)/size.y+0.5);
    float surface = surface_height(centre.xz);
    bool has_surface = surface > -999999.0;

    float random_size = hash11(fid * 31.1 + 0.8);
    float width = mix(weather.size_min_max.x,weather.size_min_max.y,random_size);
    float height = mix(weather.size_min_max.z,weather.size_min_max.w,random_size);
    vec3 quad_right = camera_right;
    vec3 quad_up = safe_normal(weather.camera_up_tan_y.xyz,vec3(0,1,0));
    float visibility = weather.collision_grid.w < 0.5 ? weather.camera_forward.w : 1.0;
    if (ground || weather.directional.w > 0.5) {
        if (!has_surface) visibility = 0.0;
        centre.y = has_surface ? surface + 0.015 : weather.wind_ground.w;
        if (blowing_snow) {
            // Snow skims above the heightfield with gust-scale lift. Rain
            // splashes keep their original ground-aligned quads.
            float lift = 0.12 + 1.1 * hash11(fid * 37.13);
            float eddy = sin(age * 2.1 + fid) * 0.12;
            centre.y += max(0.04, lift + eddy);
        }
    } else if (has_surface && centre.y < surface) {
        visibility = 0.0;
    }

    vec3 to_camera = safe_normal(weather.camera_time.xyz-centre, -camera_forward);
    if (ripple || (ground && !splash_pair && !blowing_snow)) {
        quad_right = vec3(1,0,0);
        quad_up = vec3(0,0,1);
        if (ripple) {
            width *= 1.0 + phase * 2.0;
            height *= 1.0 + phase * 2.0;
        }
    } else if (weather.directional.x > 0.0) {
        quad_up = -safe_normal(velocity + weather.camera_motion.xyz, vec3(0,-1,0));
        quad_right = safe_normal(cross(quad_up,to_camera),camera_right);
    } else {
        quad_right = safe_normal(cross(vec3(0,1,0),to_camera),camera_right);
        quad_up = safe_normal(cross(to_camera,quad_right),vec3(0,1,0));
        float angle = mix(weather.rotation.x,weather.rotation.y,hash11(fid*19.1)) * age;
        vec3 old_right = quad_right;
        quad_right = old_right*cos(angle) + quad_up*sin(angle);
        quad_up = quad_up*cos(angle) - old_right*sin(angle);
    }
    if (!ripple) {
        float z_offset = mix(weather.directional.y,weather.directional.z,hash11(fid*29.1));
        centre.y += max(height,0.002) * z_offset * 0.5;
    }
    vec2 corner = quad_corner(vertex_in_quad);
    vec3 world = centre + quad_right * corner.x * max(width,0.002)*0.5
                        + quad_up * corner.y * max(height,0.002)*0.5;
    gl_Position = weather.view_proj * vec4(world,1.0);

    float rows = max(weather.sprite_sheet.x,1.0);
    float cols = max(weather.sprite_sheet.y,1.0);
    int total_frames = int(rows*cols);
    int first = clamp(int(ripple ? weather.animation.z : weather.sprite_sheet.z),0,total_frames-1);
    int last = clamp(int(ripple ? weather.animation.w : weather.sprite_sheet.w),first,total_frames-1);
    float tex_phase = weather.animation.x > 0.0
        ? fract(mix(age,phase,clamp(weather.animation.y,0.0,1.0))*weather.animation.x)
        : hash11(fid*43.1);
    int frame = min(first + int(tex_phase * float(last-first+1)),last);
    vec2 tile = vec2(float(frame % int(cols)),float(frame / int(cols)));
    v_uv = (tile+quad_uv(vertex_in_quad))/vec2(cols,rows);
    v_uv_bounds = vec4(tile/vec2(cols,rows),(tile+1.0)/vec2(cols,rows));

    float distance_to_camera = length(world-weather.camera_time.xyz);
    float near_fade = smoothstep(max(weather.fade_near_far.x*0.5,0.0),
        max(weather.fade_near_far.x,0.001),distance_to_camera);
    float far_fade = 1.0-smoothstep(max(weather.fade_near_far.y*0.75,weather.fade_near_far.x+0.01),
        max(weather.fade_near_far.y,weather.fade_near_far.x+0.02),distance_to_camera);
    v_color = weather.color;
    v_snow = weather.velocity_min.w;
    if (blowing_snow) v_color.rgb = mix(v_color.rgb,vec3(0.90,0.94,1.0),0.8);
    // Density is driven by the draw count; avoid squaring rain strength through alpha.
    v_color.a *= visibility;
    v_life = phase;
    v_distance_fade = near_fade*far_fade;
    v_alpha_fade = min(age/max(weather.fade_near_far.z,0.001),1.0)
        * min((life-age)/max(weather.fade_near_far.w,0.001),1.0);
}
