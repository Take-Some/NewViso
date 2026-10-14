#version 450
#extension GL_GOOGLE_include_directive : require

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
    vec4 environment_reference[64];
    vec4 water_impacts[8];
    vec4 water_surface;
    vec4 water_optics;
    vec4 water_pass;
} frame;

layout(location = 0) in vec4 in_position_mode;
layout(location = 1) in vec3 in_normal;
layout(location = 2) in vec4 in_color;
layout(location = 3) in vec2 in_uv;
layout(location = 4) in vec4 in_tangent;

layout(location = 0) out vec4 v_color;
layout(location = 1) out vec3 v_normal;
layout(location = 2) out vec3 v_world_position;
layout(location = 3) out vec4 v_shadow_coord;
layout(location = 4) flat out float v_overlay;
layout(location = 5) out vec2 v_uv;
layout(location = 6) out vec4 v_tangent;

layout(set=3,binding=6,std430) readonly buffer WaterField {vec4 cells[];};
const int GRID=256;
vec4 water_at(vec2 world) {
    vec2 p=(world-frame.water_pass.zw)/2.;
    if(any(lessThan(p,vec2(0)))||any(greaterThan(p,vec2(GRID-1))))return vec4(0);
    ivec2 a=ivec2(floor(p)),b=min(a+1,ivec2(GRID-1));
    vec2 f=fract(p);
    return mix(mix(cells[a.y*GRID+a.x],cells[a.y*GRID+b.x],f.x),
               mix(cells[b.y*GRID+a.x],cells[b.y*GRID+b.x],f.x),f.y);
}
void main() {
    v_color = in_color;
    v_overlay = in_position_mode.w;
    v_uv = in_uv;
    v_tangent = in_tangent;

    if (in_position_mode.w > 0.5) {
        gl_Position = vec4(in_position_mode.xyz, 1.0);
        v_normal = vec3(0.0, 0.0, 1.0);
        v_world_position = vec3(0.0);
        v_shadow_coord = vec4(0.0);
        v_tangent = vec4(1.0, 0.0, 0.0, 1.0);
        return;
    }

    vec3 position=in_position_mode.xyz;
    vec3 normal=normalize(in_normal);
    // Preserve a per-primitive near/far flag for the optical pass.
    v_overlay=in_position_mode.w<-.5?1.:0.;
    if(in_position_mode.w<-.5)position.xz+=frame.water_pass.zw+vec2(255.);
    if(normal.y>.8) {
        vec4 water=water_at(position.xz);
        position.y=frame.water_surface.x+water.x;
        float dx=(water_at(position.xz+vec2(1,0)).x-water_at(position.xz-vec2(1,0)).x)*.5;
        float dz=(water_at(position.xz+vec2(0,1)).x-water_at(position.xz-vec2(0,1)).x)*.5;
        normal=normalize(vec3(-dx,1.,-dz));
        v_color=vec4(water.w,water.y,water.z,1.);
    }
    vec4 world = vec4(position,1.0);
    gl_Position = frame.view_proj * world;
    v_normal = normal;
    v_world_position = position;
    v_shadow_coord = frame.shadow_view_proj * world;
}
