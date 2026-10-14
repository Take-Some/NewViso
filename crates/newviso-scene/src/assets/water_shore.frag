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
layout(location=0) in vec4 v_color;
layout(location=1) in vec3 v_normal;
layout(location=2) in vec3 v_world_position;
layout(location=3) in vec4 v_shadow_coord;
layout(location=4) flat in float v_overlay;
layout(location=5) in vec2 v_uv;
layout(location=6) in vec4 v_tangent;

#define ENV_LANE(i) frame.environment_reference[i]
#include "environment.glsl"
layout(set=1,binding=0) uniform texture2D water_bump;
layout(set=1,binding=2) uniform texture2D water_foam;
layout(set=1,binding=5) uniform sampler s_material;
layout(set=3,binding=6,std430) readonly buffer WaterField {vec4 cells[];};
layout(location=0) out vec4 out_color;
vec4 wet_at(vec2 world) {
    vec2 p=(world-frame.water_pass.zw)/2.;
    if(any(lessThan(p,vec2(0)))||any(greaterThan(p,vec2(255))))return vec4(0);
    ivec2 a=ivec2(floor(p)),b=min(a+1,ivec2(255));
    vec2 f=fract(p);
    return mix(mix(cells[a.y*256+a.x],cells[a.y*256+b.x],f.x),
               mix(cells[b.y*256+a.x],cells[b.y*256+b.x],f.x),f.y);
}
void main() {
    vec4 wet=wet_at(v_world_position.xz);
    float height=v_world_position.y-frame.water_surface.x;
    if(v_overlay>.5 || height<-.015 || height>max(wet.w,wet.x)+.02 || v_normal.y<.2)discard;
    // Source water_terrainfoam: moving wet height, foam G, animated bump R.
    vec2 base=v_world_position.xz*frame.water_optics.y;
    vec2 uv=base-vec2(0,wet.w)+.25;
    float foam=texture(sampler2D(water_foam,s_material),uv).g;
    float animation=texture(sampler2D(water_bump,s_material),base+uv).r;
    float alpha=clamp(foam*animation*max(height,0.)*10.*max(wet.w*.5,0.),0.,1.);
    if(alpha<.002)discard;
    vec3 light=frame.environment_ambient.rgb*frame.environment_ambient.a;
    for(int i=0;i<clamp(int(frame.globals.x+.5),0,16);i++) {
        if(frame.light_meta[i].x>.5)continue;
        light+=frame.light_color[i].rgb*frame.light_meta[i].y*
            max(dot(normalize(v_normal),-normalize(frame.light_dir[i].xyz))*.7+.3,0.);
    }
    vec3 color=max(light,vec3(.02));
    if(environment_enabled())color=environment_apply_fog(color,frame.camera_position.xyz,v_world_position);
    out_color=vec4(color,alpha);
}
